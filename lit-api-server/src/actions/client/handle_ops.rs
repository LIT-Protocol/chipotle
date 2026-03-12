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
            UnionResponse::AesEncrypt(AesEncryptRequest { pkp_id, message }) => {
                if !op_code_helpers::can_use_wallet_in_action(&self.api_key, &self.ipfs_id, &pkp_id)
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
                if !op_code_helpers::can_use_wallet_in_action(&self.api_key, &self.ipfs_id, &pkp_id)
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
                if !op_code_helpers::can_use_wallet_in_action(&self.api_key, &self.ipfs_id, &pkp_id)
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
                let secret =
                    op_code_helpers::private_keys::get_lit_action_private_key(&self.ipfs_id)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                GetLitActionPrivateKeyResponse { secret }.into()
            }
            UnionResponse::GetLitActionPublicKey(GetLitActionPublicKeyRequest { ipfs_id }) => {
                let public_key = op_code_helpers::private_keys::get_lit_action_public_key(&ipfs_id)
                    .await
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                GetLitActionPublicKeyResponse { public_key }.into()
            }
            UnionResponse::GetLitActionWalletAddress(GetLitActionWalletAddressRequest {
                ipfs_id,
            }) => {
                let wallet_address =
                    op_code_helpers::private_keys::get_lit_action_wallet_address(&ipfs_id)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                GetLitActionWalletAddressResponse { wallet_address }.into()
            }
            UnionResponse::AddNamedOutput(AddNamedOutputRequest { name, value }) => {
                
                if self.state.named_output.len() > self.max_named_output_count as usize {
                    bail!(
                        "You may not add more than {} named outputs per session and you have attempted to exceed that limit.",
                        self.max_named_output_count
                    );
                }

                if value.len() > self.max_named_output_value_length {
                    bail!(
                        "Named output value is too long. Max length is {} bytes",
                        self.max_named_output_value_length
                    );
                }

                if name.len() > self.max_named_output_name_length {
                    bail!(
                        "Named output name is too long. Max length is {} bytes",
                        self.max_named_output_name_length
                    );
                }

                self.state.named_output.insert(name, value);
                AddNamedOutputResponse {}.into()
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
