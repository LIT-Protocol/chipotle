use rocket_okapi::okapi::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NewAccountResponse {
    pub api_key: String,
    pub wallet_address: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct HandshakeResponse {
    pub responses: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateWalletResponse {
    pub wallet_address: String,
}

/// Returned by `/create_wallet_with_signature`. The client must follow up with
/// an on-chain `registerWalletDerivation(adminHash, wallet_address, derivation_path, name, description)`
/// call signed by the same wallet — until that lands, the PKP exists in MPC but
/// is not registered to any account.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateWalletWithSignatureResponse {
    pub wallet_address: String,
    /// 0x-prefixed lowercase hex (uint256). Pass through verbatim to
    /// `registerWalletDerivation`'s `derivationPath` arg.
    pub derivation_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LitActionResponse {
    pub response: serde_json::Value,
    pub logs: String,
    pub has_error: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EncryptResponse {
    pub ciphertext: String,
    pub data_to_encrypt_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DecryptResponse {
    pub decrypted_text: String,
}

/// Response for account config operations (add_group, add_pkp_to_group, remove_pkp_from_group, add_usage_api_key, remove_usage_api_key).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddUsageApiKeyResponse {
    pub success: bool,
    pub usage_api_key: String,
}

/// Returned by `/add_usage_api_key_with_signature`. The client must follow up
/// with on-chain `registerWalletDerivation(adminHash, wallet_address, derivation_path, name, description)`
/// and `setUsageApiKey(adminHash, keccak256(usage_api_key), …)` — both signed
/// by the admin wallet — to attach the usage key to the ChainSecured account.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddUsageApiKeyWithSignatureResponse {
    /// Base64-encoded 32-byte secret. Send as `X-Api-Key` / `Authorization: Bearer …` for usage requests; pass `keccak256(this)` to `setUsageApiKey`.
    pub usage_api_key: String,
    /// 0x-prefixed lowercase hex EVM address of the minted PKP wallet.
    pub wallet_address: String,
    /// 0x-prefixed lowercase hex (uint256). Pass through verbatim to
    /// `registerWalletDerivation`'s `derivationPath` arg.
    pub derivation_path: String,
}

/// Response for account config operations (add_pkp_to_group, remove_pkp_from_group, add_usage_api_key, remove_usage_api_key).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AccountOpResponse {
    pub success: bool,
}

/// Response for add_group, includes the on-chain group ID.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddGroupResponse {
    pub success: bool,
    pub group_id: String,
}

/// Mirrors AccountConfig.sol Group struct (groupName, groupDescription, plus ids/hashes when returned).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GroupResponse {
    pub group_id: String,
    pub group_name: String,
    pub group_description: String,
}

/// One item from list_groups, list_wallets, list_wallets_in_group, or list_actions (AccountConfig.sol Metadata).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListMetadataItem {
    pub id: String, // hash of the item, as stored on chain.
    pub name: String,
    pub description: String,
}

/// One item from list_groups, list_wallets, list_wallets_in_group, or list_actions (AccountConfig.sol Metadata).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WalletItem {
    pub id: String, // hash of the item, as stored on chain.
    pub name: String,
    pub description: String,
    pub wallet_address: String, // if the item is managed by the LIT-node, this will be the actual IPFS CID, or Wallet Address, or public key, etc.
}

/// One item from list_api_keys (AccountConfig.sol UsageApiKey).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApiKeyItem {
    pub id: String,           // auto-increment metadata id, as stored on chain.
    pub api_key_hash: String, // keccak256 hash of the usage API key string (0x-prefixed hex).
    pub name: String,
    pub description: String,
    pub expiration: String,
    pub balance: u64,
    pub can_create_groups: bool,
    pub can_delete_groups: bool,
    pub can_create_pkps: bool,
    pub can_manage_ipfs_ids_in_groups: Vec<u64>,
    pub can_add_pkp_to_groups: Vec<u64>,
    pub can_remove_pkp_from_groups: Vec<u64>,
    pub can_execute_in_groups: Vec<u64>,
}

/// GET /billing/stripe_config — returns the Stripe publishable key for Stripe.js.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StripeConfigResponse {
    pub publishable_key: String,
}

/// GET /billing/balance — current credit balance for the authenticated API key.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BillingBalanceResponse {
    /// Balance in cents.  Negative means credits are available; zero means exhausted.
    pub balance_cents: i64,
    /// Human-readable, e.g. "$5.00 credit".
    pub balance_display: String,
}

/// POST /billing/create_payment_intent — client secret for Stripe.js confirmCardPayment.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreatePaymentIntentResponse {
    pub client_secret: String,
    pub payment_intent_id: String,
}

/// GET /get_chain_config_keys — names of every supported ConfigKeys variant.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChainConfigKeysResponse {
    pub keys: Vec<String>,
}

/// GET /get_lit_action_client_config — effective ClientBuilder configuration values,
/// including any chain-config overrides applied at startup.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LitActionClientConfigResponse {
    pub timeout_ms: u64,
    pub async_timeout_ms: u64,
    pub memory_limit_mb: u32,
    pub max_code_length: u64,
    pub max_response_length: u64,
    pub max_console_log_length: u64,
    pub max_fetch_count: u32,
    pub max_get_keys_count: u32,
    pub max_retries: u32,
    pub client_timeout_ms_buffer: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NodeChainConfigResponse {
    pub chain_name: String,
    pub chain_id: u64,
    pub is_evm: bool,
    pub testnet: bool,
    pub token: String,
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub rpc_url: String,
    pub contract_address: String,
}

/// One cached action-code entry in a `GET /cache_metadata` response (CPL-351).
///
/// Describes the cached data only — never the code/binary itself.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CacheEntryMetadataItem {
    /// IPFS id (cache key) of the cached action code.
    pub ipfs_id: String,
    /// Size of the cached code in bytes.
    pub size_bytes: u64,
    /// Unix-epoch milliseconds when the entry was first cached.
    pub created_at_ms: u64,
    /// Unix-epoch milliseconds of the most recent execution.
    pub last_run_at_ms: u64,
    /// Number of executions recorded against this entry.
    pub run_count: u64,
    /// Time-to-live of the entry, in seconds. `None` for the API-server IPFS
    /// cache, which is capacity-bounded (LRU) rather than time-expired.
    pub ttl_seconds: Option<u64>,
}

/// GET /cache_metadata — metadata for the cached action code correlated to the
/// authenticated master account. Excludes the cached binaries themselves.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CacheMetadataResponse {
    /// On-chain account wallet address the caller's key resolves to.
    pub account_address: String,
    /// Number of cached entries correlated to this account.
    pub entry_count: u64,
    /// Sum of `size_bytes` across the returned entries.
    pub total_size_bytes: u64,
    /// The cached entries, sorted by most recent execution first.
    pub entries: Vec<CacheEntryMetadataItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct VersionResponse {
    pub name: String,
    pub version: String,
    pub commit_version: String,
    pub submodule_versions: Vec<(String, String)>,
}

/// Returned by `/get_supported_languages` — the node's language capability
/// surface (see `actions::languages`).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SupportedLanguagesResponse {
    pub languages: Vec<crate::actions::languages::LanguageFeature>,
}
