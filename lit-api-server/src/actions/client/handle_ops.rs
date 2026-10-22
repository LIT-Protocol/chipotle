use super::op_code_helpers;
use anyhow::{Result, bail};
use lit_actions_grpc::proto::*;
use tokio::time::Instant;
use tracing::{instrument, warn};

use super::Client;
use super::models::ExecutionState;

/// How many seconds of execution to accumulate before flushing a charge to Stripe.
const BILLING_FLUSH_INTERVAL_SECS: u64 = 5;

impl Client {
    #[instrument(level = "debug", skip(self, op), err)]
    pub async fn handle_op(
        &mut self,
        op: UnionResponse,
        call_depth: u32,
    ) -> Result<ExecuteJsRequest> {
        // NOTE: Do not log `op` here — response variants (GetPrivateKeyResponse,
        // GetLitActionPrivateKeyResponse, AesDecryptResponse) carry secret material.
        self.state.ops_count += 1;
        // Identity for CID-derived key material and PKP-wallet authorization:
        // the bundle checksum (`ipfs_id`).
        let key_id = self.action_key_id().to_string();
        let op_type = match &op {
            UnionResponse::SetResponse(_) => "SetResponse",
            UnionResponse::Print(_) => "Print",
            UnionResponse::IncrementFetchCount(_) => "IncrementFetchCount",
            UnionResponse::AesEncrypt(_) => "AesEncrypt",
            UnionResponse::AesDecrypt(_) => "AesDecrypt",
            UnionResponse::GetPrivateKey(_) => "GetPrivateKey",
            UnionResponse::GetLitActionPrivateKey(_) => "GetLitActionPrivateKey",
            UnionResponse::GetLitActionPublicKey(_) => "GetLitActionPublicKey",
            UnionResponse::GetLitActionWalletAddress(_) => "GetLitActionWalletAddress",
            UnionResponse::UpdateResourceUsage(_) => "UpdateResourceUsage",
            UnionResponse::Result(_) => "Result",
        };
        tracing::debug!(op_type, ops_count = self.state.ops_count, "handle_op");

        Ok(match op {
            UnionResponse::SetResponse(SetResponseRequest { response }) => {
                if response.len() > self.max_response_length as usize {
                    bail!(
                        "Response is too long. Max length is {} bytes",
                        self.max_response_length
                    );
                }
                self.state.response = response;
                SetResponseResponse {}.into()
            }
            UnionResponse::Print(PrintRequest { message }) => {
                if self.state.logs.len() + message.len() > self.max_console_log_length as usize {
                    bail!(
                        "Console.log is printing something that is too long. Max length for all logs in a single request is {} bytes",
                        self.max_console_log_length
                    );
                }
                self.state.logs.push_str(&message);
                PrintResponse {}.into()
            }
            UnionResponse::IncrementFetchCount(IncrementFetchCountRequest {}) => {
                self.state.fetch_count += 1;
                // self.pay(LitActionPriceComponent::Fetches, 1).await?;
                if self.state.fetch_count > self.max_fetch_count {
                    bail!(
                        "You may not send more than {} HTTP requests per session and you have attempted to exceed that limit.",
                        self.max_fetch_count
                    );
                }
                IncrementFetchCountResponse {
                    fetch_count: self.state.fetch_count,
                }
                .into()
            }
            UnionResponse::AesEncrypt(AesEncryptRequest { pkp_id, message }) => {
                if !op_code_helpers::can_use_wallet_in_action(&self.api_key, &key_id, &pkp_id)
                    .await?
                {
                    bail!("API key cannot use selected wallet in selected action");
                }

                let encrypted = op_code_helpers::encryption::aes_encrypt_with_pkp(
                    &self.api_key,
                    &pkp_id,
                    &message,
                )
                .await?;
                AesEncryptResponse {
                    ciphertext: encrypted,
                }
                .into()
            }
            UnionResponse::AesDecrypt(AesDecryptRequest { pkp_id, ciphertext }) => {
                if !op_code_helpers::can_use_wallet_in_action(&self.api_key, &key_id, &pkp_id)
                    .await?
                {
                    bail!("API key cannot use selected wallet in selected action");
                }

                let decrypted = op_code_helpers::encryption::aes_decrypt_with_pkp(
                    &self.api_key,
                    &pkp_id,
                    &ciphertext,
                )
                .await?;
                AesDecryptResponse {
                    plaintext: decrypted,
                }
                .into()
            }
            UnionResponse::GetPrivateKey(GetPrivateKeyRequest { pkp_id }) => {
                self.check_get_keys_limit()?;
                if !op_code_helpers::can_use_wallet_in_action(&self.api_key, &key_id, &pkp_id)
                    .await?
                {
                    bail!("API key cannot use selected wallet in selected action");
                }
                let secret = op_code_helpers::private_keys::get_private_key(&self.api_key, &pkp_id)
                    .await
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                GetPrivateKeyResponse { secret }.into()
            }
            UnionResponse::GetLitActionPrivateKey(GetLitActionPrivateKeyRequest {}) => {
                self.check_get_keys_limit()?;
                let secret = op_code_helpers::private_keys::get_lit_action_private_key(&key_id)
                    .await
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                GetLitActionPrivateKeyResponse { secret }.into()
            }
            UnionResponse::GetLitActionPublicKey(GetLitActionPublicKeyRequest { ipfs_id }) => {
                self.check_get_keys_limit()?;
                let public_key = op_code_helpers::private_keys::get_lit_action_public_key(&ipfs_id)
                    .await
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                GetLitActionPublicKeyResponse { public_key }.into()
            }
            UnionResponse::GetLitActionWalletAddress(GetLitActionWalletAddressRequest {
                ipfs_id,
            }) => {
                self.check_get_keys_limit()?;
                let wallet_address =
                    op_code_helpers::private_keys::get_lit_action_wallet_address(&ipfs_id)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                GetLitActionWalletAddressResponse { wallet_address }.into()
            }
            UnionResponse::UpdateResourceUsage(UpdateResourceUsageRequest {
                tick: _,
                used_kb: _,
            }) => {
                let cancel_action = if let Some(ref stripe) = self.stripe_state {
                    let api_key = self.api_key.as_str();
                    // Derived CID of the action — always set on the billed
                    // execution path; `charge` normalizes an empty CID
                    // away (see execution.rs).
                    let ipfs_id = self.ipfs_id.as_str();
                    let request_id = self.request_id.as_deref();
                    bill_tick(
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
                } else {
                    false
                };
                UpdateResourceUsageResponse { cancel_action }.into()
            }
            UnionResponse::Result(_) => unreachable!(), // handled in main loop
        })
    }

    /// Increments the key-op counter and bails if the per-session
    /// `max_get_keys_count` limit is exceeded. Called by every key-returning
    /// handler (GetPrivateKey, GetLitActionPrivateKey, GetLitActionPublicKey,
    /// GetLitActionWalletAddress).
    fn check_get_keys_limit(&mut self) -> Result<()> {
        self.state.get_keys_count += 1;
        if self.state.get_keys_count > self.max_get_keys_count {
            bail!(
                "You may not perform more than {} key operations per session and you have attempted to exceed that limit.",
                self.max_get_keys_count
            );
        }
        Ok(())
    }
}

/// Bill a runtime tick: accrue the wall-clock seconds since the last billed
/// second and charge them through `charge` once [`BILLING_FLUSH_INTERVAL_SECS`]
/// have accumulated. Returns whether to cancel the action because the charge
/// failed.
pub(super) async fn bill_tick<F, Fut>(state: &mut ExecutionState, now: Instant, charge: F) -> bool
where
    F: FnOnce(u64) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    state.accrue_elapsed_seconds(now);
    if state.unbilled_seconds < BILLING_FLUSH_INTERVAL_SECS {
        return false;
    }
    match charge(state.unbilled_seconds).await {
        Ok(()) => {
            state.unbilled_seconds = 0;
            false
        }
        Err(e) => {
            // `charge` is all-or-nothing, so nothing was charged. Keep the
            // seconds: the end-of-run flush settles them and, if the balance
            // can't cover them, charges what it does cover. Dropping them here
            // let a run that blocked past its balance and then yielded finish
            // without paying anything.
            warn!("Per-second billing failed, cancelling action: {e}");
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::client::ClientBuilder;
    use std::future::ready;
    use tokio::time::Duration;

    #[tokio::test]
    async fn tick_charges_every_flush_interval() {
        let start = Instant::now();
        let mut state = ExecutionState {
            execution_start: Some(start),
            ..Default::default()
        };
        let mut charged = Vec::new();
        let cancel = bill_tick(&mut state, start + Duration::from_millis(4_900), |s| {
            charged.push(s);
            ready(Ok(()))
        })
        .await;
        assert!(!cancel);
        assert!(charged.is_empty(), "under the flush interval: {charged:?}");
        let cancel = bill_tick(&mut state, start + Duration::from_millis(5_100), |s| {
            charged.push(s);
            ready(Ok(()))
        })
        .await;
        assert!(!cancel);
        assert_eq!(charged, [5]);
        assert_eq!(state.unbilled_seconds, 0);
    }

    #[tokio::test]
    async fn refused_tick_keeps_the_debt_and_cancels() {
        // The action blocked for 60 s, then yielded. The balance can't cover
        // the 60 s, so the action is cancelled and the 60 s are left for the
        // end-of-run flush to settle.
        let start = Instant::now();
        let mut state = ExecutionState {
            execution_start: Some(start),
            ..Default::default()
        };
        let cancel = bill_tick(&mut state, start + Duration::from_millis(60_200), |_| {
            ready(Err(anyhow::anyhow!("insufficient credits")))
        })
        .await;
        assert!(cancel);
        assert_eq!(state.unbilled_seconds, 60);
    }

    #[test]
    fn get_keys_limit_allows_up_to_max_then_bails() {
        let limit = 3u32;
        let mut client = ClientBuilder::default()
            .max_get_keys_count(limit)
            .build()
            .unwrap();

        // The first `limit` key ops are allowed.
        for i in 1..=limit {
            assert!(
                client.check_get_keys_limit().is_ok(),
                "key op #{i} should be within the limit of {limit}"
            );
            assert_eq!(client.state.get_keys_count, i);
        }

        // The next one exceeds the limit and must bail.
        let err = client
            .check_get_keys_limit()
            .expect_err("exceeding the limit should error");
        assert!(
            err.to_string().contains(&limit.to_string()),
            "error should mention the limit: {err}"
        );
        // The counter still advanced past the limit.
        assert_eq!(client.state.get_keys_count, limit + 1);
    }
}
