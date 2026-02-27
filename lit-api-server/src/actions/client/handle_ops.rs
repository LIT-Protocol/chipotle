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
            UnionResponse::AesEncryptToAction(AesEncryptToActionRequest { message, ipfs_id }) => {
                let encrypted = op_code_helpers::encryption::aes_encrypt_to_action_with_pkp(
                    &self.api_key,
                    &message,
                    &ipfs_id,
                )
                .await?;
                AesEncryptToActionResponse {
                    ciphertext: encrypted,
                    ipfs_id: ipfs_id.clone(),
                }
                .into()
            }
            UnionResponse::AesDecryptToAction(AesDecryptToActionRequest { ciphertext }) => {
                let ipfs_id = &self.state.ipfs_id;
                let decrypted = op_code_helpers::encryption::aes_decrypt_to_action_with_pkp(
                    &self.api_key,
                    &ciphertext,
                    ipfs_id,
                )
                .await?;
                AesDecryptToActionResponse {
                    plaintext: decrypted,
                    ipfs_id: String::new(), // current action ipfs_id if available
                }
                .into()
            }
            UnionResponse::GetLatestNonce(GetLatestNonceRequest { .. }) => {
                bail!("GetLatestNonce is not implemented");
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
            UnionResponse::CallContract(CallContractRequest { .. }) => {
                bail!("CallContract is not implemented");
            }
            UnionResponse::GetRpcUrl(GetRpcUrlRequest { .. }) => {
                bail!("GetRpcUrl is not implemented");
            }
            UnionResponse::EncryptBls(EncryptBlsRequest {
                access_control_conditions: _,
                to_encrypt: _,
            }) => {
                // use lit_rust_crypto::blsful::Bls12381G1;

                bail!("EncryptBls is not implemented");
            }
            UnionResponse::DecryptBls(DecryptBlsRequest { .. }) => {
                bail!("DecryptBls is not implemented");
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
            UnionResponse::PubkeyToTokenId(_) => {
                bail!("PubkeyToTokenId is not implemented");
            }
            UnionResponse::PkpPermissionsGetPermitted(_) => {
                bail!("PkpPermissionsGetPermitted is not implemented");
            }
            UnionResponse::PkpPermissionsGetPermittedAuthMethodScopes(_) => {
                bail!("PkpPermissionsGetPermittedAuthMethodScopes is not implemented");
            }
            UnionResponse::PkpPermissionsIsPermitted(_) => {
                bail!("PkpPermissionsIsPermitted is not implemented");
            }
            UnionResponse::PkpPermissionsIsPermittedAuthMethod(_) => {
                bail!("PkpPermissionsIsPermittedAuthMethod is not implemented");
            }
            UnionResponse::SignEcdsa(_) => {
                bail!("SignEcdsa is not implemented");
            }
            UnionResponse::CheckConditions(_) => {
                bail!("CheckConditions is not implemented");
            }
            UnionResponse::ClaimKeyIdentifier(_) => {
                bail!("ClaimKeyIdentifier is not implemented");
            }
            UnionResponse::BroadcastAndCollect(_) => {
                bail!("BroadcastAndCollect is not implemented");
            }
            UnionResponse::DecryptAndCombine(_) => {
                bail!("DecryptAndCombine is not implemented");
            }
            UnionResponse::SignAndCombineEcdsa(_) => {
                bail!("SignAndCombineEcdsa is not implemented");
            }
            UnionResponse::P2pBroadcast(_) => {
                bail!("P2pBroadcast is not implemented");
            }
            UnionResponse::P2pCollectFromLeader(_) => {
                bail!("P2pCollectFromLeader is not implemented");
            }
            UnionResponse::IsLeader(_) => {
                bail!("IsLeader is not implemented");
            }
            UnionResponse::DecryptToSingleNode(_) => {
                bail!("DecryptToSingleNode is not implemented");
            }
            UnionResponse::SignAndCombine(_) => {
                bail!("SignAndCombine is not implemented");
            }
            UnionResponse::Result(_) => unreachable!(), // handled in main loop
        })
    }
}
