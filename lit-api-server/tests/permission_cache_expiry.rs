//! A cached grant must not outlive the calling usage key's on-chain
//! expiration, however often it is read. Exercises the real operation
//! dispatcher and `accounts::can_execute_action` against the shared
//! permission cache without a chain: a permission lookup that reaches the
//! chain fails with the uninitialised-client error, which proves the cache
//! did not serve the grant.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alloy::primitives::Address;
use lit_actions_grpc::proto::{AesEncryptRequest, UnionResponse};
use lit_api_server::accounts::blockchain_cache::{self, CachedPermission, GrantDeadline};
use lit_api_server::accounts::can_execute_action;
use lit_api_server::actions::client::{Client, ClientBuilder};
use lit_api_server::utils::parse_with_hash::{api_key_hash, ipfs_cid_to_u256};

const CID: &str = "QmYRKz97sdQ2DwG4w3uoTKSzycX2DDGQvuUQJuZN1UFdDR";
const WALLET: &str = "0x0000000000000000000000000000000000000104";
const NO_CLIENT: &str = "Read-only client not initialised — call init_chain_clients() at startup";

/// Lifetime left on the expiring key when the test starts.
const LIFETIME: Duration = Duration::from_millis(400);
/// How long the test keeps using both keys after the deadline.
const AFTER: Duration = Duration::from_millis(300);
/// Requests finishing this close to the deadline may go either way.
const MARGIN: Duration = Duration::from_millis(50);

fn now() -> Duration {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap()
}

fn client(api_key: &str) -> Client {
    blockchain_cache::init();
    ClientBuilder::default()
        .api_key(api_key)
        .ipfs_id(CID)
        .build()
        .unwrap()
}

/// Cache a wallet grant for `api_key`, as a lookup before expiry would.
async fn grant_wallet(api_key: &str, deadline: GrantDeadline) {
    let cache = blockchain_cache::get().unwrap();
    let key = cache.use_wallet_key(
        api_key_hash(api_key),
        ipfs_cid_to_u256(CID).unwrap(),
        WALLET.parse::<Address>().unwrap(),
    );
    cache
        .use_wallet_cache()
        .insert(key, CachedPermission::new(true, deadline))
        .await;
}

/// Cache an execution grant for `api_key`, as a lookup before expiry would.
async fn grant_execution(api_key: &str, deadline: GrantDeadline) {
    blockchain_cache::init();
    let cache = blockchain_cache::get().unwrap();
    let key = cache.execute_action_key(api_key_hash(api_key), ipfs_cid_to_u256(CID).unwrap());
    cache
        .execute_action_cache()
        .insert(key, CachedPermission::new(true, deadline))
        .await;
}

/// Encrypt with the wallet; the error shows where the operation stopped.
async fn encrypt_error(client: &mut Client) -> String {
    let op = UnionResponse::AesEncrypt(AesEncryptRequest {
        pkp_id: WALLET.into(),
        message: "test".into(),
    });
    client.handle_op(op, 0).await.unwrap_err().to_string()
}

/// The permission check was served from the cache, so the operation reached
/// wallet derivation, which fails without a chain client.
fn reached_derivation(err: &str) -> bool {
    err.contains(NO_CLIENT) && err != NO_CLIENT
}

#[tokio::test]
async fn sustained_wallet_use_stops_at_usage_key_expiration() {
    let (expiring_key, never_key) = ("issue-165-expiring-wallet", "issue-165-never-wallet");
    let mut expiring = client(expiring_key);
    let mut never = client(never_key);
    let deadline = now() + LIFETIME;
    grant_wallet(expiring_key, GrantDeadline::At(deadline)).await;
    grant_wallet(never_key, GrantDeadline::Never).await;

    let (mut served, mut refused) = (0, 0);
    while now() < deadline + AFTER {
        let started = now();
        let err = encrypt_error(&mut expiring).await;
        if started >= deadline {
            assert_eq!(
                err,
                NO_CLIENT,
                "expired grant served from the cache {:?} after the deadline",
                started - deadline
            );
            refused += 1;
        } else if now() + MARGIN < deadline {
            assert!(reached_derivation(&err), "live grant not served: {err}");
            served += 1;
        }
        // Reads keep renewing the grant of a key that never expires.
        let err = encrypt_error(&mut never).await;
        assert!(reached_derivation(&err), "never-expiring grant lost: {err}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        served > 0 && refused > 0,
        "{served} served, {refused} refused"
    );
}

#[tokio::test]
async fn sustained_execution_checks_stop_at_usage_key_expiration() {
    let (expiring_key, never_key) = ("issue-165-expiring-exec", "issue-165-never-exec");
    let cid = ipfs_cid_to_u256(CID).unwrap();
    let deadline = now() + LIFETIME;
    grant_execution(expiring_key, GrantDeadline::At(deadline)).await;
    grant_execution(never_key, GrantDeadline::Never).await;

    let (mut served, mut refused) = (0, 0);
    while now() < deadline + AFTER {
        let started = now();
        let result = can_execute_action(expiring_key, cid).await;
        if started >= deadline {
            let err = result.expect_err("expired execution grant served from the cache");
            assert_eq!(err.to_string(), NO_CLIENT);
            refused += 1;
        } else if now() + MARGIN < deadline {
            assert!(result.unwrap(), "live execution grant not served");
            served += 1;
        }
        assert!(can_execute_action(never_key, cid).await.unwrap());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        served > 0 && refused > 0,
        "{served} served, {refused} refused"
    );
}
