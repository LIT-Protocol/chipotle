use hmac::{Hmac, Mac};
use lit_agent_keychain::{
    sponsorship::{decrypt_key, encrypt_key},
    stripe::verify_signature,
    subscriptions::{Subscription, FREE_LIMIT, STANDARD_LIMIT},
};
use sha2::Sha256;
use time::{Duration, OffsetDateTime};

#[test]
fn subscription_requires_paid_active_period_and_custom_access_expires() {
    let now = OffsetDateTime::now_utc();
    let mut sub = Subscription {
        customer_id: None,
        subscription_id: None,
        status: "active".into(),
        paid_until: Some(now + Duration::days(1)),
        cancel_at_period_end: true,
        checkout_id: None,
        checkout_generation: 0,
        custom_secret_limit: None,
        custom_until: None,
    };
    assert!(sub.plan(now).active); // cancellation at period end preserves paid access
    assert_eq!(sub.plan(now).plan, "standard");
    assert_eq!(sub.plan(now).secret_limit, 1000);
    assert_eq!(STANDARD_LIMIT, 1000);
    assert_eq!(FREE_LIMIT, 5);
    for status in [
        "none",
        "incomplete",
        "incomplete_expired",
        "past_due",
        "unpaid",
        "canceled",
        "paused",
        "trialing",
    ] {
        sub.status = status.into();
        assert!(!sub.plan(now).active, "{status}");
        // Unpaid vaults fall back to Free rather than losing storage entirely.
        assert_eq!(sub.plan(now).plan, "free", "{status}");
        assert_eq!(sub.plan(now).secret_limit, FREE_LIMIT, "{status}");
    }
    sub.status = "active".into();
    sub.paid_until = Some(now);
    assert!(!sub.plan(now).active);
    assert_eq!(sub.plan(now).secret_limit, FREE_LIMIT);
    sub.custom_secret_limit = Some(2500);
    sub.custom_until = Some(now + Duration::days(1));
    assert!(sub.plan(now).active);
    assert_eq!(sub.plan(now).plan, "custom");
    assert_eq!(sub.plan(now).secret_limit, 2500);
    sub.custom_until = Some(now);
    assert!(!sub.plan(now).active);
    assert_eq!(sub.plan(now).secret_limit, FREE_LIMIT);
}
#[test]
fn stripe_signature_binds_original_bytes_and_timestamp_and_accepts_rotating_signatures() {
    let secret = "whsec_test";
    let body = b"{\"event\":1}";
    let now = 1700000000;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{now}.").as_bytes());
    mac.update(body);
    let signature = hex::encode(mac.finalize().into_bytes());
    let header = format!("t={now},v1={},v1={signature}", "00".repeat(32));
    verify_signature(secret, &header, body, now).unwrap();
    assert!(verify_signature(secret, &header, b"{\"event\":2}", now).is_err());
    assert!(verify_signature("wrong", &header, body, now).is_err());
    assert!(verify_signature(secret, &header, body, now + 301).is_err());
    assert!(verify_signature(secret, &header, body, now - 301).is_err());
    assert!(verify_signature(secret, &format!("t={now},{header}"), body, now).is_err());
}
#[test]
fn encrypted_usage_credentials_cannot_be_swapped_between_vaults() {
    let secret = [7u8; 32];
    let key = "test-usage-credential";
    let encrypted = encrypt_key(key, "vault-a", &secret).unwrap();
    assert_ne!(encrypted, encrypt_key(key, "vault-a", &secret).unwrap());
    assert_eq!(decrypt_key(&encrypted, "vault-a", &secret).unwrap(), key);
    assert!(decrypt_key(&encrypted, "vault-b", &secret).is_err());
    assert!(decrypt_key(&encrypted, "vault-a", &[8u8; 32]).is_err());
    let mut bytes = hex::decode(encrypted).unwrap();
    bytes[15] ^= 1;
    assert!(decrypt_key(&hex::encode(bytes), "vault-a", &secret).is_err());
}
