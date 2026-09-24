use lit_agent_keychain::discovery::validate_challenge;
use serde_json::{json, Value};

use ed25519_dalek::{Signer, SigningKey};
use lit_agent_keychain::discovery::{active_grant, verify_proof};

#[test]
fn discovery_signature_rejects_wrong_keys_and_tampering() {
    let key = SigningKey::from_bytes(&[7; 32]);
    let other = SigningKey::from_bytes(&[8; 32]);
    let now = 1_790_000_000;
    let c = json!({"v":2,"domain":"lit-keychain/discovery/v2","audience":"https://keychain.example","agentPublicKey":hex::encode(key.verifying_key().as_bytes()),"nonce":"22".repeat(32),"issuedAt":now,"expiresAt":now+60});
    let hash = hex::decode(lit_agent_keychain::crypto::digest(&c).unwrap()).unwrap();
    let sig = hex::encode(key.sign(&hash).to_bytes());
    assert!(verify_proof(&c, &sig, "https://keychain.example", now).is_ok());
    assert!(verify_proof(
        &c,
        &hex::encode(other.sign(&hash).to_bytes()),
        "https://keychain.example",
        now
    )
    .is_err());
    for field in ["nonce", "agentPublicKey", "audience"] {
        let mut changed = c.clone();
        changed[field] = json!("33".repeat(32));
        assert!(verify_proof(&changed, &sig, "https://keychain.example", now).is_err());
    }
    assert!(verify_proof(&c, &"00".repeat(64), "https://keychain.example", now).is_err());
}

#[test]
fn discovery_current_grants_match_operation_version_hash_and_time() {
    let p = json!({"v":2,"domain":"lit-keychain/v2","disabled":false,"notBefore":100,"expiresAt":200,"grants":[{"agentPublicKey":"A","operations":["get"],"versions":[{"version":1,"envelopeHash":"hash"}]}]});
    assert!(active_grant(&p, "A", "get", 1, "hash", 150));
    assert!(!active_grant(&p, "B", "get", 1, "hash", 150));
    assert!(!active_grant(&p, "A", "stripe.balance", 1, "hash", 150));
    assert!(!active_grant(&p, "A", "get", 2, "hash", 150));
    assert!(!active_grant(&p, "A", "get", 1, "other", 150));
    assert!(!active_grant(&p, "A", "get", 1, "hash", 99));
    assert!(!active_grant(&p, "A", "get", 1, "hash", 200));
    let mut disabled = p.clone();
    disabled["disabled"] = json!(true);
    assert!(!active_grant(&disabled, "A", "get", 1, "hash", 150));
    let mut indefinite = p.clone();
    indefinite["expiresAt"] = Value::Null;
    assert!(active_grant(&indefinite, "A", "get", 1, "hash", 10000));
}

#[test]
fn discovery_challenges_bind_audience_identity_and_short_window() {
    let now = 1_790_000_000;
    let original = json!({"v":2,"domain":"lit-keychain/discovery/v2","audience":"https://keychain.example","agentPublicKey":"11".repeat(32),"nonce":"22".repeat(32),"issuedAt":now,"expiresAt":now+60});
    assert!(validate_challenge(&original, "https://keychain.example", now).is_ok());
    assert!(validate_challenge(&original, "https://other.example", now).is_err());
    assert!(validate_challenge(&original, "https://keychain.example", now + 60).is_err());
    for (field, value) in [
        ("v", json!(1)),
        ("domain", json!("lit-keychain/v2")),
        ("nonce", json!("bad")),
        ("agentPublicKey", json!("bad")),
        ("expiresAt", json!(now + 61)),
        ("issuedAt", json!(now + 31)),
        ("extra", json!(true)),
    ] {
        let mut changed = original.clone();
        changed[field] = value;
        assert!(
            validate_challenge(&changed, "https://keychain.example", now).is_err(),
            "{field}"
        );
    }
}
