use ed25519_dalek::{Signer, SigningKey};
use lit_agent_keychain::{
    crypto,
    replay::{self, Proof},
};
use serde_json::json;

fn proof(now: i64, vault: &str, nonce: &str) -> Proof {
    let key = SigningKey::from_bytes(&[19; 32]);
    let request = json!({"v":2,"domain":"lit-keychain/v2","vaultId":vault,
        "secretId":"22".repeat(32),"agentPublicKey":hex::encode(key.verifying_key().as_bytes()),
        "nonce":nonce,"operation":"get","policyHash":"33".repeat(32),
        "envelopeHash":"44".repeat(32),"issuedAt":now,"expiresAt":now+90});
    let signature = hex::encode(
        key.sign(&hex::decode(crypto::digest(&request).unwrap()).unwrap())
            .to_bytes(),
    );
    Proof { request, signature }
}
#[test]
fn request_proof_binds_nonce_and_validity() {
    let now = 1_790_000_000;
    let p = proof(now, &"11".repeat(32), &"55".repeat(32));
    assert!(replay::verify(&p, now).is_ok());
    assert!(replay::verify(&p, now + 90).is_err());
    assert!(replay::verify(&p, now - 31).is_err());
    for field in [
        "nonce",
        "vaultId",
        "agentPublicKey",
        "operation",
        "expiresAt",
    ] {
        let mut changed = proof(now, &"11".repeat(32), &"55".repeat(32));
        changed.request[field] = json!("66".repeat(32));
        assert!(replay::verify(&changed, now).is_err());
    }
}
#[tokio::test]
async fn concurrent_claims_are_single_use_and_survive_reconnection() {
    let Ok(url) = std::env::var("KEYCHAIN_TEST_DATABASE_URL") else {
        return;
    };
    assert!(url.contains("localhost") || url.contains("127.0.0.1"));
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    lit_agent_keychain::db::run_migrations(&pool).await.unwrap();
    let vault = crypto::random_token();
    let nonce = crypto::random_token();
    sqlx::query("INSERT INTO kc_vaults(id,authority,authority_cid) VALUES($1,'{}','test')")
        .bind(&vault)
        .execute(&pool)
        .await
        .unwrap();
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut tasks = Vec::new();
    for _ in 0..20 {
        let pool = pool.clone();
        let p = proof(now, &vault, &nonce);
        tasks.push(tokio::spawn(async move {
            replay::claim(&pool, &p).await.unwrap()
        }));
    }
    let mut accepted = 0;
    for task in tasks {
        accepted += usize::from(task.await.unwrap());
    }
    assert_eq!(accepted, 1);
    pool.close().await;
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    assert!(!replay::claim(&pool, &proof(now, &vault, &nonce))
        .await
        .unwrap());
    assert!(
        replay::claim(&pool, &proof(now, &vault, &crypto::random_token()))
            .await
            .unwrap()
    );
    assert!(
        replay::claim(&pool, &proof(now - 120, &vault, &crypto::random_token()))
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM kc_audit WHERE vault_id=$1 AND event='agent_request_consumed'",
    )
    .bind(&vault)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 2);
    sqlx::query("DELETE FROM kc_audit WHERE vault_id=$1")
        .bind(&vault)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM kc_vaults WHERE id=$1")
        .bind(&vault)
        .execute(&pool)
        .await
        .unwrap();
}
