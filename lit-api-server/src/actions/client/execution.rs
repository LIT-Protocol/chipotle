use super::models::{ExecutionOptions, ExecutionState};

use crate::error::{Error, unexpected_err};
use crate::stripe::{BillingError, COST_LIT_ACTION_PER_SECOND_CENTS};
use anyhow::{Context, Result, anyhow, bail};
use std::path::PathBuf;
use tracing::warn;

use crate::actions::jobs::{ActionJob, ActionStore, JobId};
use futures::{FutureExt as _, TryFutureExt};
use lit_actions_grpc::tokio_stream::StreamExt as _;
use lit_actions_grpc::tonic::{
    Code, Extensions, Request, Status, transport::Error as TransportError,
};

use super::Client;
use lit_actions_grpc::{proto::*, unix};
use tokio::time::{Duration, Instant};
use tracing::instrument;

impl Client {
    /// Bill the rest of the run to Stripe: all wall-clock time since the last
    /// charged second, not only the seconds runtime ticks reported (a
    /// synchronous loop blocks the runtime's event loop and sends no ticks).
    /// Called at the end of execution to ensure no time goes unbilled.
    /// Enforces a minimum charge of 1 second per action.
    /// Returns `Err` if the charge fails — the caller should abort the action.
    async fn flush_unbilled_seconds(&mut self) -> Result<()> {
        let Some(ref stripe) = self.stripe_state else {
            return Ok(());
        };
        let api_key = self.api_key.as_str();
        // Derived CID of the action — always set on the billed execution
        // path (raw code is hashed the same way registered actions are).
        // `charge` normalizes an empty CID away, so no filtering here.
        let ipfs_id = self.ipfs_id.as_str();
        let request_id = self.request_id.as_deref();
        settle_run(
            &mut self.state,
            tokio::time::Instant::now(),
            move |seconds| {
                crate::stripe::charge_lit_action_time(
                    api_key,
                    seconds,
                    Some(ipfs_id),
                    request_id,
                    stripe,
                )
            },
        )
        .await
    }

    #[instrument(level = "debug", skip_all, ret)]
    pub async fn execute_js_async(
        &self,
        opts: impl Into<ExecutionOptions>,
        store: &ActionStore,
    ) -> Result<JobId> {
        store
            .clone()
            .submit_job(ActionJob::new(
                Client {
                    timeout_ms: self.async_timeout_ms,
                    ..self.clone()
                },
                opts,
            ))
            .await
    }

    // NOTE: `ret` intentionally omitted — ExecutionState contains `response` and `logs`
    // which are set by user JS code and may include secret material.
    #[instrument(level = "debug", skip_all, err)]
    pub async fn execute_js(
        &mut self,
        opts: impl Into<ExecutionOptions>,
    ) -> Result<ExecutionState, Error> {
        self.reset_state();
        let opts = opts.into();
        let timeout = self.client_timeout();

        // Hand-roll retry loop as crates like tokio-retry or again don't play well with &mut self
        let mut retry = 0;
        loop {
            let execution = Box::pin(self.execute_js_inner(
                opts.code.clone(),
                opts.globals.clone(),
                // &auth_context,
                0,
            ));
            let execution_result = match tokio::time::timeout(timeout, execution).await {
                Ok(result) => result,
                Err(_) => {
                    // Flush any accumulated billing before returning the timeout error.
                    if let Err(e) = self.flush_unbilled_seconds().await {
                        warn!("Billing flush failed on timeout: {e}");
                    }
                    return Err(unexpected_err(
                        anyhow!("timeout"),
                        Some(format!("lit_actions didn't respond within {timeout:?}")),
                    ));
                }
            };
            match execution_result {
                Ok(state) => return Ok(state),
                Err(e) => {
                    let last_error = if let Some(status) = e.downcast_ref::<Status>() {
                        let msg = status.message().to_string();
                        match status.code() {
                            Code::DeadlineExceeded => {
                                return Err(unexpected_err(
                                    e,
                                    Some("tonic deadline exceeded error".to_string()),
                                ));
                            }
                            Code::ResourceExhausted => {
                                return Err(unexpected_err(
                                    e,
                                    Some("tonic resource exhausted error".to_string()),
                                ));
                            }
                            Code::Unavailable => {
                                // This error occurs when NGINX can't connect to any healthy
                                // lit_actions instance and returns the gRPC status code 14
                                return Err(unexpected_err(
                                    e,
                                    Some("tonic unavailable error".to_string()),
                                ));
                            }
                            // NB: We could also retry on `Code::Internal if msg == "h2 protocol error: error reading a body from connection"`.
                            // However, that likely means lit_actions has crashed *while* executing JS, which we can't recover from.
                            _ => return Err(unexpected_err(e, Some(msg.to_string()))),
                        }
                    } else if let Some(te) = e.downcast_ref::<TransportError>() {
                        // This error occurs when the socket file is missing or lit_actions is down
                        // - connection error: No such file or directory (os error 2)
                        // - connection error: Connection refused (os error 61)
                        anyhow!("tonic transport error: {:?}", te) // te.source().unwrap_or(te).to_string())
                    } else if let Some(se) = e.downcast_ref::<flume::SendError<ExecuteJsRequest>>()
                    {
                        // This error occurs when NGINX can't connect to any healthy lit_actions instance
                        // - connection error: sending on a closed channel
                        anyhow!(
                            "tonic send error: {:?}",
                            se // se.source().unwrap_or(se).to_string()
                        )
                    } else {
                        return Err(unexpected_err(e, None));
                    };

                    // Never retry in-flight requests, which may have modified state
                    if retry >= self.max_retries || self.state.ops_count != 0 {
                        return Err(unexpected_err(last_error, None));
                    }
                    let backoff = Duration::from_secs(2u64.pow(retry));
                    tracing::error!("Retrying execute_js in {backoff:?}, cause: {last_error:?}");
                    tokio::time::sleep(backoff).await;
                    retry += 1;
                }
            }
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    pub(crate) async fn execute_js_inner(
        &mut self,
        code: String,
        globals: Option<serde_json::Value>,
        // auth_context: &models::AuthContext,
        call_depth: u32,
    ) -> Result<ExecutionState> {
        let js_params_bytes = globals
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .context("failed to serialize js_params")?;
        let js_params_len = js_params_bytes.as_ref().map_or(0, Vec::len);
        let combined_len = code.len() + js_params_len;
        if combined_len > self.max_code_length as usize {
            bail!(
                "Combined code + js_params payload is too large ({} bytes: {} code + {} js_params). Max combined size is {} bytes.",
                combined_len,
                code.len(),
                js_params_len,
                self.max_code_length,
            );
        }

        let (outbound_tx, outbound_rx) = flume::bounded(0);

        // The JS route leaves `socket_path` unset and falls back to the JS
        // runner's socket.
        let socket_path = self
            .socket_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(crate::core::v1::health::LIT_ACTIONS_SOCKET));
        tracing::debug!("execution::grpc_channel: connecting");
        let channel = self
            .client_grpc_channels
            .create_or_get_connection(&socket_path.display().to_string(), || {
                unix::connect_to_socket(socket_path)
                    .map_err(|e| anyhow!("Error creating connection to lit-action server: {:?}", e))
            })
            .await?;
        tracing::debug!("execution::grpc_stream_setup: opening bidirectional stream");
        let mut stream = ActionClient::new(channel)
            .execute_js(Request::from_parts(
                self.metadata()?,
                Extensions::default(),
                outbound_rx.into_stream(),
            ))
            // Fix "implementation of `std::marker::Send` is not general enough"
            // Workaround for compiler bug https://github.com/rust-lang/rust/issues/96865
            // See https://github.com/rust-lang/rust/issues/100013#issuecomment-2052045872
            .boxed()
            .await?
            .into_inner();
        tracing::debug!("execution::grpc_stream_setup: stream established");

        // Start the billing clock before execution begins.
        self.state.execution_start = Some(tokio::time::Instant::now());
        self.state.execution_end = None;

        // Send initial execution request to server
        outbound_tx
            .send_async(
                ExecutionRequest {
                    code: code.to_string(),
                    js_params: js_params_bytes,
                    auth_context: serde_json::to_vec(&[0; 0]).ok(),
                    http_headers: self.http_headers.clone(),
                    timeout: Some(self.timeout_ms),
                    memory_limit: Some(self.memory_limit_mb),
                    ipfs_id: if self.ipfs_id.is_empty() {
                        None
                    } else {
                        Some(self.ipfs_id.clone())
                    },
                }
                .into(),
            )
            .await
            .context("failed to send execution request")?;

        // Handle responses from server.
        // Wrap the loop in a closure so we can always flush billing on exit,
        // even if stream reads or op sends fail mid-flight.
        let loop_result: Result<ExecutionState> = async {
            while let Some(resp) = stream.try_next().await? {
                match resp.union {
                    // Return final result from server
                    Some(UnionResponse::Result(res)) => {
                        // NOTE: Side effects (signatures, contract calls) have already
                        // executed by this point. If billing fails here, the caller gets
                        // an error but the effects are not reversible. This is an accepted
                        // tradeoff — the alternative (pre-billing) was removed in favour
                        // of per-second metering.
                        self.flush_unbilled_seconds().await?;
                        if !res.success {
                            bail!(res.error);
                        }
                        // Return current state, which might be updated by subsequent code executions
                        return Ok(self.state.clone());
                    }
                    // Handle op requests
                    Some(op) => {
                        let resp = self.handle_op(op, call_depth).await.unwrap_or_else(|e| {
                            ErrorResponse {
                                error: e.to_string(),
                            }
                            .into()
                        });
                        outbound_tx
                            .send_async(resp)
                            .await
                            .context("failed to send op response")?;
                    }
                    // Ignore empty responses
                    None => {}
                };
            }
            bail!("Server unexpectedly closed connection")
        }
        .await;

        // If the loop exited via an error (stream failure, send failure, connection
        // close), do a best-effort billing flush before propagating the error.
        if loop_result.is_err()
            && let Err(e) = self.flush_unbilled_seconds().await
        {
            warn!("Billing flush failed after error exit: {e}");
        }
        loop_result
    }
}

/// Settle the end of a run: charge, through `charge`, every second not yet
/// charged (see [`ExecutionState::seconds_to_flush`]), then mark the run
/// settled. Returns `Err` if the charge fails — the caller should abort the
/// action.
async fn settle_run<F, Fut>(state: &mut ExecutionState, now: Instant, charge: F) -> Result<()>
where
    F: FnMut(u64) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let seconds = state.seconds_to_flush(now);
    if seconds == 0 {
        return Ok(());
    }
    tracing::debug!(seconds, "execution::flush_unbilled_seconds: charging");
    if let Err(FinalChargeError {
        error,
        balance_exhausted,
    }) = charge_finished_run(seconds, charge).await
    {
        warn!("Failed to bill remaining {seconds} seconds at end of action: {error}");
        if balance_exhausted {
            // What the balance covered is charged; the rest can't be
            // collected, so drop it.
            state.settle_flush();
        }
        bail!("Billing failed for {seconds} seconds of execution: {error}");
    }
    state.settle_flush();
    Ok(())
}

/// Why the end-of-run charge failed.
struct FinalChargeError {
    error: anyhow::Error,
    /// The balance could not cover the run. Whatever it did cover has been
    /// charged; the remainder cannot be collected.
    balance_exhausted: bool,
}

/// Charge `seconds` of a finished run through `charge`. If the balance can't
/// cover all of them, charge the seconds it does cover instead of nothing: the
/// compute has already run, and a refused charge leaves the balance untouched,
/// which would keep admitting the account to further unpaid runs.
async fn charge_finished_run<F, Fut>(seconds: u64, mut charge: F) -> Result<(), FinalChargeError>
where
    F: FnMut(u64) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let Err(error) = charge(seconds).await else {
        return Ok(());
    };
    let Some(mut affordable) = affordable_seconds(&error) else {
        // Not a balance refusal (e.g. Stripe unavailable): keep the debt so the
        // caller's next flush can retry it.
        return Err(FinalChargeError {
            error,
            balance_exhausted: false,
        });
    };
    // Another request for the same customer can spend between the refusal and
    // this charge, so retry at the balance each refusal reports.
    for _ in 0..3 {
        if affordable == 0 {
            // Nothing left to collect.
            return Err(FinalChargeError {
                error,
                balance_exhausted: true,
            });
        }
        match charge(affordable).await {
            Ok(()) => {
                return Err(FinalChargeError {
                    error,
                    balance_exhausted: true,
                });
            }
            Err(e) => match affordable_seconds(&e) {
                Some(now_affordable) => affordable = now_affordable,
                None => {
                    warn!(
                        "Failed to bill the remaining balance ({affordable} seconds) at end of action: {e}"
                    );
                    break;
                }
            },
        }
    }
    // Nothing was charged: keep the debt for the caller's next flush.
    Err(FinalChargeError {
        error,
        balance_exhausted: false,
    })
}

/// The seconds the account can still pay for, if `error` refused a charge for
/// insufficient credit.
fn affordable_seconds(error: &anyhow::Error) -> Option<u64> {
    match error.downcast_ref::<BillingError>()? {
        BillingError::InsufficientCredits {
            available_cents, ..
        } => Some(u64::try_from(available_cents / COST_LIT_ACTION_PER_SECOND_CENTS).unwrap_or(0)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::future::{Ready, ready};

    fn insufficient(available_cents: i64, seconds: u64) -> anyhow::Error {
        anyhow::Error::new(BillingError::InsufficientCredits {
            available_cents,
            required_cents: seconds as i64 * COST_LIT_ACTION_PER_SECOND_CENTS,
        })
    }

    fn unavailable() -> anyhow::Error {
        anyhow::Error::new(BillingError::Unavailable(anyhow!("stripe down")))
    }

    /// A customer balance charged the way `stripe::charge` charges it: the
    /// whole amount, or nothing and an `InsufficientCredits` refusal.
    struct Account {
        cents: Cell<i64>,
        attempts: RefCell<Vec<u64>>,
    }

    impl Account {
        fn holding(cents: i64) -> Self {
            Self {
                cents: Cell::new(cents),
                attempts: RefCell::default(),
            }
        }

        fn charge(&self, seconds: u64) -> Ready<Result<()>> {
            self.attempts.borrow_mut().push(seconds);
            let available = self.cents.get();
            let cost = seconds as i64 * COST_LIT_ACTION_PER_SECOND_CENTS;
            if cost > available {
                return ready(Err(insufficient(available, seconds)));
            }
            self.cents.set(available - cost);
            ready(Ok(()))
        }

        fn attempts(&self) -> Vec<u64> {
            self.attempts.borrow().clone()
        }
    }

    fn started_at(start: Instant) -> ExecutionState {
        ExecutionState {
            execution_start: Some(start),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn busy_loop_is_billed_for_its_wall_clock_time() {
        // `while (true) {}` sends no ticks and is killed at a 900 s timeout.
        let start = Instant::now();
        let mut state = started_at(start);
        let account = Account::holding(10_000);
        let end = start + Duration::from_millis(900_400);
        settle_run(&mut state, end, |s| account.charge(s))
            .await
            .unwrap();
        assert_eq!(account.attempts(), [900]);
        assert_eq!(account.cents.get(), 9_100);
    }

    #[tokio::test]
    async fn failed_action_is_settled_once() {
        // A failed action is flushed when its result arrives (2.1 s) and again
        // on the error exit, after the first charge took until 3.2 s.
        let start = Instant::now();
        let mut state = started_at(start);
        let account = Account::holding(10_000);
        settle_run(&mut state, start + Duration::from_millis(2_100), |s| {
            account.charge(s)
        })
        .await
        .unwrap();
        settle_run(&mut state, start + Duration::from_millis(3_200), |s| {
            account.charge(s)
        })
        .await
        .unwrap();
        assert_eq!(account.attempts(), [2]);
    }

    #[tokio::test]
    async fn blocking_past_the_balance_then_yielding_pays_what_it_can() {
        // An account holding 2 cents blocks for 60 s, then yields. The tick
        // that follows is refused and cancels the action; the end-of-run flush
        // then charges the 2 cents instead of nothing, so the account can't
        // start another run.
        let start = Instant::now();
        let mut state = started_at(start);
        let account = Account::holding(2);
        let cancel = super::super::handle_ops::bill_tick(
            &mut state,
            start + Duration::from_millis(60_200),
            |s| account.charge(s),
        )
        .await;
        assert!(cancel);
        let settled = settle_run(&mut state, start + Duration::from_millis(60_300), |s| {
            account.charge(s)
        })
        .await;
        assert!(settled.is_err());
        // The error exit flushes again; there is nothing left to charge.
        settle_run(&mut state, start + Duration::from_millis(61_000), |s| {
            account.charge(s)
        })
        .await
        .unwrap();
        assert_eq!(account.attempts(), [60, 60, 2]);
        assert_eq!(account.cents.get(), 0);
    }

    #[tokio::test]
    async fn short_balance_is_charged_instead_of_nothing() {
        // A 900-second run on an account holding 37 cents: the full charge is
        // refused, so the 37 seconds the balance covers are charged instead.
        let account = Account::holding(37);
        let result = charge_finished_run(900, |s| account.charge(s)).await;
        assert_eq!(account.attempts(), [900, 37]);
        assert_eq!(account.cents.get(), 0);
        assert!(result.err().is_some_and(|e| e.balance_exhausted));
    }

    #[tokio::test]
    async fn drain_follows_a_balance_another_request_spent() {
        let account = Account::holding(37);
        let result = charge_finished_run(900, |s| {
            let charged = account.charge(s);
            if account.attempts().len() == 1 {
                // Another request for the customer spends a cent before the
                // drain, so the drain at 37 is refused too.
                account.cents.set(account.cents.get() - 1);
            }
            charged
        })
        .await;
        assert_eq!(account.attempts(), [900, 37, 36]);
        assert_eq!(account.cents.get(), 0);
        assert!(result.err().is_some_and(|e| e.balance_exhausted));
    }

    #[tokio::test]
    async fn empty_balance_is_not_charged_again() {
        let account = Account::holding(0);
        let result = charge_finished_run(900, |s| account.charge(s)).await;
        assert_eq!(account.attempts(), [900]);
        assert!(result.err().is_some_and(|e| e.balance_exhausted));
    }

    #[tokio::test]
    async fn unavailable_billing_keeps_the_debt() {
        let mut attempts = Vec::new();
        let result = charge_finished_run(900, |seconds| {
            attempts.push(seconds);
            ready(Err(unavailable()))
        })
        .await;
        assert_eq!(attempts, [900]);
        assert!(result.err().is_some_and(|e| !e.balance_exhausted));
    }

    #[tokio::test]
    async fn failed_drain_keeps_the_debt() {
        // Billing becomes unavailable between the refusal and the drain, so
        // nothing is charged and the next flush must retry the whole debt.
        let mut attempts = Vec::new();
        let result = charge_finished_run(900, |seconds| {
            attempts.push(seconds);
            ready(Err(if attempts.len() == 1 {
                insufficient(37, seconds)
            } else {
                unavailable()
            }))
        })
        .await;
        assert_eq!(attempts, [900, 37]);
        assert!(result.err().is_some_and(|e| !e.balance_exhausted));
    }

    #[tokio::test]
    async fn covered_run_is_charged_once() {
        let account = Account::holding(10_000);
        let result = charge_finished_run(900, |s| account.charge(s)).await;
        assert_eq!(account.attempts(), [900]);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn drain_keeps_the_debt_after_three_refusals() {
        // Another request spends a cent before every drain attempt, so each
        // is refused. Nothing was charged, so the debt is kept.
        let account = Account::holding(37);
        let result = charge_finished_run(900, |s| {
            let charged = account.charge(s);
            account.cents.set(account.cents.get() - 1);
            charged
        })
        .await;
        assert_eq!(account.attempts(), [900, 37, 36, 35]);
        assert!(result.err().is_some_and(|e| !e.balance_exhausted));
    }

    #[tokio::test]
    async fn unavailable_charge_is_retried_by_the_next_flush() {
        // Billing is unavailable when the result arrives (3.4 s). The error
        // exit's flush, after a 30 s Stripe timeout, retries the same 3 s.
        let start = Instant::now();
        let mut state = started_at(start);
        let account = Account::holding(10_000);
        let first = settle_run(&mut state, start + Duration::from_millis(3_400), |s| {
            account.attempts.borrow_mut().push(s);
            ready(Err(unavailable()))
        })
        .await;
        assert!(first.is_err());
        settle_run(&mut state, start + Duration::from_millis(33_400), |s| {
            account.charge(s)
        })
        .await
        .unwrap();
        assert_eq!(account.attempts(), [3, 3]);
        assert_eq!(account.cents.get(), 9_997);
    }
}
