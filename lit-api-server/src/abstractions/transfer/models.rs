use serde::{Deserialize, Serialize};

use crate::abstractions::transfer::chain_info::Chain;

/// Request body for the send endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRequest {
    pub api_key: String,
    pub pkp_public_key: String,
    pub chain: String,
    pub destination_address: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferResponse {
    pub txn_id: String,
    pub success: bool,
    pub chain: Chain,
    pub origin_symbol: String,
    pub origin_amount: f64,
    pub gas: String,
    pub timestamp: String,
    pub destination_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBalanceResponse {
    pub address: String,
    pub balance: f64,
    pub chain: Chain,
    pub symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainInfoResponse {
    pub chain: String,
    pub display_name: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetChainsResponse {
    pub chains: Vec<ChainInfoResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetChainsRequest {
    pub is_evm: bool,
    pub is_testnet: bool,
}
