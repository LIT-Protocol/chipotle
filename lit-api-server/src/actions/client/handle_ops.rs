use super::op_code_helpers;
use anyhow::{Result, bail};
use lit_actions_grpc::proto::*;
use tracing::{instrument, trace};

use super::Client;

impl Client {
    #[instrument(level = "debug", skip(self), err)]
    pub async fn handle_op(
        &mut self,
        op: UnionResponse,
        call_depth: u32,
    ) -> Result<ExecuteJsRequest> {
        trace!("handle_op: {:?}", op);
        self.state.ops_count += 1;

        Ok(match op {
            UnionResponse::SetResponse(SetResponseRequest { response }) => {
                if response.len() > self.max_response_length {
                    bail!(
                        "Response is too long. Max length is {} bytes",
                        self.max_response_length
                    );
                }
                self.state.response = response;
                SetResponseResponse {}.into()
            }
            UnionResponse::Print(PrintRequest { message }) => {
                if self.state.logs.len() + message.len() > self.max_console_log_length {
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
            UnionResponse::AesEncrypt(AesEncryptRequest {
                public_key,
                message,
            }) => {
                let encrypted = op_code_helpers::encryption::aes_encrypt_with_pkp(
                    &self.api_key,
                    &public_key,
                    &message,
                )
                .await?;
                AesEncryptResponse {
                    ciphertext: encrypted,
                }
                .into()
            }
            UnionResponse::AesDecrypt(AesDecryptRequest {
                public_key,
                ciphertext,
            }) => {
                let decrypted = op_code_helpers::encryption::aes_decrypt_with_pkp(
                    &self.api_key,
                    &public_key,
                    &ciphertext,
                )
                .await?;
                AesDecryptResponse {
                    plaintext: decrypted,
                }
                .into()
            }
            UnionResponse::Sign(SignRequest {
                to_sign,
                public_key,
                sig_name,
                signing_scheme,
            }) => {
                let (sig_name, signed_data) = match op_code_helpers::signing::sign_with_pkp(
                    &self.api_key,
                    &public_key,
                    &to_sign,
                    &sig_name,
                    &signing_scheme,
                )
                .await
                {
                    Ok((sig_name, signed_data)) => (sig_name, signed_data),
                    Err(e) => bail!("Error signing: {:?}", e),
                };

                let hex_signature = signed_data.signature.clone();

                self.state.sign_count += 1;
                self.state.signed_data.insert(sig_name, signed_data);

                SignResponse {
                    success: hex_signature,
                }
                .into()
            }
            UnionResponse::CallChild(CallChildRequest {
                ipfs_id: _,
                params: _,
            }) => {
                bail!("CallChild is not implemented - missing IPFS caching");

                // // self.pay(LitActionPriceComponent::CallDepth, 1).await?;

                // tracing::info!(
                //     "Calling child action: {:?}",
                //     ipfs_id
                // );
                // call_depth += 1;
                // if call_depth > self.max_call_depth {
                //     bail!(
                //         "The recursion limit of a child action is {} and you have attempted to exceed that limit.",
                //         self.max_call_depth
                //     );
                // }

                // TODO: Implement IPFS caching - or pull from Lit-Peer?
                // Pull down the lit action code from IPFS
                // let code = crate::utils::web::get_ipfs_file(
                //     &ipfs_id,
                //     self.lit_config(),
                //     self.ipfs_cache()?,
                //     self.http_cache()?,
                // )
                // .await?;

                // let globals = params
                //     .map(|params| serde_json::from_slice::<serde_json::Value>(&params))
                //     .transpose()?;

                // // NB: Using execute_js_inner instead of execute_js to avoid resetting state
                // let res = Box::pin(self.execute_js_inner(code, globals,  call_depth))
                //     .await?;

                // CallChildResponse {
                //     response: res.response,
                // }
                // .into()
            }
            UnionResponse::CallContract(CallContractRequest { .. }) => {
                bail!("CallContract is not implemented");
            }
            UnionResponse::UpdateResourceUsage(UpdateResourceUsageRequest {
                tick: _,
                used_kb: _,
            }) => {
                // // For now, we'll just return a success response
                // let r = self
                //     .dynamic_payment
                //     .add(LitActionPriceComponent::MemoryUsage, used_kb as u64);
                // let cancel_action = r.is_err();
                let cancel_action = false;
                UpdateResourceUsageResponse { cancel_action }.into()
            }
            UnionResponse::Result(_) => unreachable!(), // handled in main loop
        })
    }
}
