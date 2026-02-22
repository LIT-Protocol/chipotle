use anyhow::{Result, bail};
use lit_core::utils::binary::bytes_to_hex;
use lit_rust_crypto::k256::ecdsa::SigningKey;

use crate::actions::client::models::SignedData;
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
            UnionResponse::SignEcdsa(SignEcdsaRequest {
                to_sign,
                public_key, // currently this is actually the wallet address.
                sig_name,
                eth_personal_sign: _,
                key_set_id: _,
            }) => {
                let api_key = self.api_key.clone();
                let secret_u256 =
                    crate::accounts::get_wallet_derivation(&api_key, &public_key).await?;
                let mut secret_bytes = [0; 32];
                secret_u256.to_big_endian(&mut secret_bytes);

                let signing_key = SigningKey::from_slice(&secret_bytes)?;
                let signature = signing_key.sign_recoverable(&to_sign)?;
                let hex_signature = bytes_to_hex(&signature.0.to_vec());

                self.state.sign_count += 1;
                self.state.signed_data.insert(
                    sig_name,
                    SignedData {
                        signing_scheme: "EcdsaK256Sha256".to_string(),
                        digest: bytes_to_hex(&to_sign),
                        public_key: public_key,
                        signature: hex_signature.clone(),
                    },
                );

                // let recovery_id = signature.1.to_string();

                SignEcdsaResponse {
                    success: hex_signature,
                }
                .into()
            }
            UnionResponse::Sign(SignRequest {
                to_sign: _,
                public_key: _,
                sig_name: _,
                signing_scheme: _,
                key_set_id: _,
            }) => {
                bail!("Sign is not implemented");
            }
            UnionResponse::CallChild(CallChildRequest {
                ipfs_id: _,
                params: _,
            }) => {
                // self.pay(LitActionPriceComponent::CallDepth, 1).await?;

                // info!(
                //     "Calling child action: {:?}, self keyset id: {:?}",
                //     ipfs_id, self.key_set_id
                // );
                // call_depth += 1;
                // if call_depth > self.max_call_depth {
                //     bail!(
                //         "The recursion limit of a child action is {} and you have attempted to exceed that limit.",
                //         self.max_call_depth
                //     );
                // }

                // // Pull down the lit action code from IPFS
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

                // let auth_context = {
                //     let mut ctx = auth_context.clone();
                //     ctx.action_ipfs_id_stack.push(ipfs_id.clone());
                //     ctx
                // };

                // // NB: Using execute_js_inner instead of execute_js to avoid resetting state
                // let res = Box::pin(self.execute_js_inner(code, globals, &auth_context, call_depth))
                //     .await?;

                // CallChildResponse {
                //     response: res.response,
                // }
                // .into()
                bail!("CallChild is not implemented");
            }

            UnionResponse::EncryptBls(EncryptBlsRequest {
                access_control_conditions: _,
                to_encrypt: _,
                key_set_id: _,
            }) => {
                // use lit_rust_crypto::blsful::Bls12381G1;

                bail!("EncryptBls is not implemented");
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
            UnionResponse::SignAsAction(SignAsActionRequest {
                to_sign: _,
                sig_name: _,
                signing_scheme: _,
            }) => {
                bail!("SignAsAction is not implemented");
            }
            UnionResponse::GetActionPublicKey(GetActionPublicKeyRequest {
                signing_scheme: _,
                action_ipfs_cid: _,
            }) => {
                bail!("GetActionPublicKey is not implemented");
            }
            UnionResponse::VerifyActionSignature(VerifyActionSignatureRequest {
                signing_scheme: _,
                action_ipfs_cid: _,
                to_sign: _,
                sign_output: _,
            }) => {
                bail!("VerifyActionSignature is not implemented");
            }
            UnionResponse::Result(_) => unreachable!(), // handled in main loop
            _ => bail!("Unhandled operation: {:?}", op),
        })
    }
}
