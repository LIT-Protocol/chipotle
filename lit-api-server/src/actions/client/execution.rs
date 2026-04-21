use super::models::{ExecutionOptions, ExecutionState};

use crate::error::{Error, unexpected_err};
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
use tokio::time::Duration;
use tracing::instrument;

impl Client {
    /// Bill any remaining accumulated seconds to Stripe.
    /// Called at the end of execution to ensure no time goes unbilled.
    /// Enforces a minimum charge of 1 second per action.
    /// Returns `Err` if the charge fails — the caller should abort the action.
    async fn flush_unbilled_seconds(&mut self) -> Result<()> {
        if let Some(ref stripe) = self.stripe_state {
            // Minimum billed amount for any action is 1 second.
            let seconds = self
                .state
                .unbilled_seconds
                .max(if self.state.last_billed_second == 0 {
                    1
                } else {
                    0
                });
            if seconds == 0 {
                return Ok(());
            }
            tracing::debug!(seconds, "execution::flush_unbilled_seconds: charging");
            crate::stripe::charge_lit_action_time(&self.api_key, seconds, stripe)
                .await
                .map_err(|e| {
                    warn!("Failed to bill remaining {seconds} seconds at end of action: {e}");
                    anyhow!("Billing failed for {seconds} seconds of execution: {e}")
                })?;
            self.state.unbilled_seconds = 0;
        }
        Ok(())
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
        if code.len() > self.max_code_length as usize {
            bail!(
                "Code payload is too large ({} bytes). Max length is {} bytes.",
                code.len(),
                self.max_code_length,
            );
        }

        let (outbound_tx, outbound_rx) = flume::bounded(0);

        // let socket_path = self.socket_path();
        let socket_path = PathBuf::from("/tmp/lit_actions.sock");
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

        // Send initial execution request to server
        outbound_tx
            .send_async(
                ExecutionRequest {
                    code: code.to_string(),
                    js_params: globals.and_then(|v| serde_json::to_vec(&v).ok()),
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
