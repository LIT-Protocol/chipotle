//! Exercise the real operation dispatcher and shared permission cache without
//! a chain or TEE. Grants reach the (deliberately uninitialized) derivation
//! client; denials must stop at authorization instead. No key is released.
use alloy::primitives::Address;
use lit_actions_grpc::proto::{
    AesDecryptRequest, AesEncryptRequest, GetPrivateKeyRequest, UnionResponse,
};
use lit_api_server::accounts::blockchain_cache;
use lit_api_server::actions::client::{Client, ClientBuilder};
use lit_api_server::utils::parse_with_hash::{api_key_hash, ipfs_cid_to_u256};

const CID: &str = "QmYRKz97sdQ2DwG4w3uoTKSzycX2DDGQvuUQJuZN1UFdDR";
const WALLET: &str = "0x0000000000000000000000000000000000000104";
const DENIED: &str = "API key cannot use selected wallet in selected action";
const NO_CLIENT: &str = "Read-only client not initialised — call init_chain_clients() at startup";

fn client(api_key: &str) -> Client {
    blockchain_cache::init();
    ClientBuilder::default()
        .api_key(api_key)
        .ipfs_id(CID)
        .build()
        .unwrap()
}

fn cache_key(api_key: &str) -> String {
    blockchain_cache::get().unwrap().use_wallet_key(
        api_key_hash(api_key),
        ipfs_cid_to_u256(CID).unwrap(),
        WALLET.parse::<Address>().unwrap(),
    )
}

async fn store(api_key: &str, permitted: bool) {
    blockchain_cache::get()
        .unwrap()
        .use_wallet_cache()
        .insert(cache_key(api_key), permitted)
        .await;
}

fn key_op() -> UnionResponse {
    UnionResponse::GetPrivateKey(GetPrivateKeyRequest {
        pkp_id: WALLET.into(),
    })
}

async fn op_error(client: &mut Client, op: UnionResponse) -> String {
    client.handle_op(op, 0).await.unwrap_err().to_string()
}

async fn assert_reaches_derivation(client: &mut Client, op: UnionResponse) {
    let err = op_error(client, op).await;
    assert!(
        err.contains(NO_CLIENT),
        "expected derivation failure: {err}"
    );
    assert_ne!(err, NO_CLIENT, "permission lookup must hit the warm cache");
}

async fn revocation_stops_op(api_key: &str, op: impl Fn() -> UnionResponse) {
    let mut execution = client(api_key);
    store(api_key, true).await;
    assert_reaches_derivation(&mut execution, op()).await;

    // This is the same selective invalidation entry point used by the existing
    // account-event listener. Another request has fetched the revoked result.
    blockchain_cache::invalidate_for_hash(api_key_hash(api_key));
    store(api_key, false).await;
    assert_eq!(op_error(&mut execution, op()).await, DENIED);
}

#[tokio::test]
async fn in_flight_private_key_observes_revocation() {
    revocation_stops_op("issue-104-private-key", key_op).await;
}

#[tokio::test]
async fn in_flight_aes_encrypt_observes_revocation() {
    revocation_stops_op("issue-104-encrypt", || {
        UnionResponse::AesEncrypt(AesEncryptRequest {
            pkp_id: WALLET.into(),
            message: "test".into(),
        })
    })
    .await;
}

#[tokio::test]
async fn in_flight_aes_decrypt_observes_revocation() {
    revocation_stops_op("issue-104-decrypt", || {
        UnionResponse::AesDecrypt(AesDecryptRequest {
            pkp_id: WALLET.into(),
            ciphertext: "unused".into(),
        })
    })
    .await;
}

#[tokio::test]
async fn failed_refetch_does_not_reuse_execution_grant() {
    let api_key = "issue-104-failed-refetch";
    let mut execution = client(api_key);
    store(api_key, true).await;
    assert_reaches_derivation(&mut execution, key_op()).await;
    blockchain_cache::invalidate_for_key(api_key);

    // A miss must fail at authorization, not reuse the old execution grant and
    // proceed to derivation. Failed lookups must not be cached either.
    for _ in 0..2 {
        assert_eq!(op_error(&mut execution, key_op()).await, NO_CLIENT);
        assert_eq!(
            blockchain_cache::get()
                .unwrap()
                .use_wallet_cache()
                .get(&cache_key(api_key))
                .await,
            None
        );
    }
}

#[tokio::test]
async fn shared_cache_eviction_is_not_hidden_by_execution_grant() {
    let api_key = "issue-104-eviction";
    let mut execution = client(api_key);
    store(api_key, true).await;
    assert_reaches_derivation(&mut execution, key_op()).await;
    // Model the cache miss after expiry/eviction without a five-minute sleep.
    blockchain_cache::get()
        .unwrap()
        .use_wallet_cache()
        .invalidate(&cache_key(api_key))
        .await;
    assert_eq!(op_error(&mut execution, key_op()).await, NO_CLIENT);
}

#[tokio::test]
async fn in_flight_denial_does_not_hide_new_grant() {
    let api_key = "issue-104-new-grant";
    let mut execution = client(api_key);
    store(api_key, false).await;
    assert_eq!(op_error(&mut execution, key_op()).await, DENIED);
    blockchain_cache::invalidate_for_hash(api_key_hash(api_key));
    store(api_key, true).await;
    assert_reaches_derivation(&mut execution, key_op()).await;
}

#[tokio::test]
async fn late_old_generation_fill_cannot_authorize_next_operation() {
    let api_key = "issue-104-late-fill";
    let unrelated = "issue-104-unrelated";
    let mut execution = client(api_key);
    let mut other_execution = client(unrelated);
    store(api_key, true).await;
    store(unrelated, true).await;
    assert_reaches_derivation(&mut execution, key_op()).await;
    let old_key = cache_key(api_key);
    blockchain_cache::invalidate_for_hash(api_key_hash(api_key));
    store(api_key, false).await;
    // An RPC begun before invalidation may finish afterward. It only fills the
    // old generation; subsequent operations must use the new generation.
    blockchain_cache::get()
        .unwrap()
        .use_wallet_cache()
        .insert(old_key, true)
        .await;
    assert_eq!(op_error(&mut execution, key_op()).await, DENIED);
    assert_reaches_derivation(&mut other_execution, key_op()).await;
}
