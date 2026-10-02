//! Server-side port of `lit-static/dapps/dashboard/password-protocol.js`:
//! the versioned wire format shared by the browser crypto worker and this
//! service. KDF costs are fixed here and never caller-selected.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::crypto::is_hex;
use super::{Failure, fail};

pub const FORMAT: i64 = 1;
pub const CIPHER: &str = "AES-256-GCM";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Kdf {
    pub algorithm: &'static str,
    pub version: i64,
    pub memory: i64,
    pub iterations: i64,
    pub parallelism: i64,
}

pub const KDF: Kdf = Kdf {
    algorithm: "argon2id",
    version: 19,
    memory: 65536,
    iterations: 3,
    parallelism: 4,
};

/// What the browser needs to derive keys for one credential version.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Parameters {
    pub format: i64,
    pub environment: String,
    pub id: String,
    pub salt: String,
    pub version: i64,
    pub kdf: Kdf,
}

pub fn parameters(environment: &str, id: &str, salt: &str, version: i64) -> Parameters {
    Parameters {
        format: FORMAT,
        environment: environment.to_string(),
        id: id.to_string(),
        salt: salt.to_string(),
        version,
        kdf: KDF,
    }
}

/// Browser-encrypted account record. Field order is the stored JSON order.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Envelope {
    pub format: i64,
    pub cipher: String,
    pub environment: String,
    pub id: String,
    pub salt: String,
    pub version: i64,
    pub account: String,
    pub nonce: String,
    pub ciphertext: String,
}

pub fn valid_environment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 40
        && value
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'))
}

pub fn validate_parameters(p: &Parameters) -> Result<(), &'static str> {
    if p.format != FORMAT || !valid_environment(&p.environment) {
        return Err("Unsupported encryption format.");
    }
    if !is_hex(&p.id, 16) || !is_hex(&p.salt, 16) {
        return Err("Invalid encrypted record.");
    }
    if p.kdf != KDF {
        return Err("Unsupported password derivation parameters.");
    }
    if p.version < 1 {
        return Err("Invalid credential version.");
    }
    Ok(())
}

fn str_field<'a>(e: &'a Value, key: &str) -> Option<&'a str> {
    e.get(key).and_then(Value::as_str)
}

/// Validate an envelope the client uploaded against the parameters the server
/// expects for it. Returns the typed record (unknown fields are dropped).
pub fn validate_envelope(e: &Value, p: &Parameters) -> Result<Envelope, String> {
    validate_parameters(p).map_err(str::to_string)?;
    let Some(object) = e.as_object() else {
        return Err("Encrypted record does not match this account.".into());
    };
    let format = object.get("format").and_then(Value::as_i64);
    let version = object.get("version").and_then(Value::as_i64);
    if format != Some(FORMAT)
        || str_field(e, "cipher") != Some(CIPHER)
        || str_field(e, "id") != Some(p.id.as_str())
        || str_field(e, "environment") != Some(p.environment.as_str())
        || version != Some(p.version)
        || str_field(e, "salt") != Some(p.salt.as_str())
    {
        return Err("Encrypted record does not match this account.".into());
    }
    let account = str_field(e, "account").unwrap_or_default();
    if !valid_account(account) {
        return Err("Invalid account binding.".into());
    }
    let nonce = str_field(e, "nonce").unwrap_or_default();
    let ciphertext = str_field(e, "ciphertext").unwrap_or_default();
    // Current API key is base64 of 32 bytes (44 UTF-8 bytes) plus a 16-byte GCM tag.
    if !is_hex(nonce, 12) || !is_hex(ciphertext, 60) {
        return Err("Invalid encrypted record.".into());
    }
    Ok(Envelope {
        format: FORMAT,
        cipher: CIPHER.to_string(),
        environment: p.environment.clone(),
        id: p.id.clone(),
        salt: p.salt.clone(),
        version: p.version,
        account: account.to_string(),
        nonce: nonce.to_string(),
        ciphertext: ciphertext.to_string(),
    })
}

/// `^0x[0-9a-f]{40}$`
pub fn valid_account(value: &str) -> bool {
    value.len() == 42 && value.starts_with("0x") && is_hex(&value[2..], 20)
}

/// Normalise and validate an email field: trimmed, lowercased, ≤254 chars,
/// `^[^\s@]+@[^\s@]+\.[^\s@]+$`.
pub fn email(value: Option<&Value>) -> Result<String, Failure> {
    let Some(raw) = value.and_then(Value::as_str) else {
        return fail(400, "Enter a valid email address.");
    };
    if raw.len() > 254 {
        return fail(400, "Enter a valid email address.");
    }
    let trimmed = raw.trim();
    let Some((local, domain)) = trimmed.split_once('@') else {
        return fail(400, "Enter a valid email address.");
    };
    let no_ws_or_at = |s: &str| !s.is_empty() && !s.chars().any(|c| c.is_whitespace() || c == '@');
    let Some((label, tld)) = domain.rsplit_once('.') else {
        return fail(400, "Enter a valid email address.");
    };
    if !no_ws_or_at(local) || !no_ws_or_at(label) || !no_ws_or_at(tld) {
        return fail(400, "Enter a valid email address.");
    }
    Ok(trimmed.to_lowercase())
}

/// A bearer token or auth secret: exactly 32 bytes of lowercase hex.
pub fn token(value: Option<&Value>) -> Result<String, Failure> {
    match value.and_then(Value::as_str) {
        Some(v) if is_hex(v, 32) => Ok(v.to_string()),
        _ => fail(400, "Invalid request."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn params() -> Parameters {
        parameters("test", &"11".repeat(16), &"22".repeat(16), 1)
    }

    fn envelope() -> Value {
        json!({
            "format": 1, "cipher": "AES-256-GCM", "environment": "test",
            "id": "11".repeat(16), "salt": "22".repeat(16), "version": 1,
            "account": format!("0x{}", "12".repeat(20)),
            "nonce": "33".repeat(12), "ciphertext": "44".repeat(60),
            "extra": "ignored"
        })
    }

    #[test]
    fn accepts_matching_envelope_and_drops_unknown_fields() {
        let e = validate_envelope(&envelope(), &params()).unwrap();
        assert_eq!(e.account, format!("0x{}", "12".repeat(20)));
        let stored = serde_json::to_string(&e).unwrap();
        assert!(!stored.contains("extra"));
        assert!(stored.starts_with("{\"format\":1,\"cipher\":\"AES-256-GCM\""));
    }

    #[test]
    fn rejects_mismatches() {
        let p = params();
        for (key, value) in [
            ("format", json!(2)),
            ("cipher", json!("AES-128-GCM")),
            ("id", json!("ff".repeat(16))),
            ("environment", json!("other")),
            ("version", json!(2)),
            ("salt", json!("ff".repeat(16))),
            ("account", json!(format!("0x{}", "AB".repeat(20)))),
            ("account", json!("0x1234")),
            ("nonce", json!("33".repeat(11))),
            ("ciphertext", json!("44".repeat(61))),
            ("ciphertext", json!(44)),
        ] {
            let mut e = envelope();
            e[key] = value;
            assert!(
                validate_envelope(&e, &p).is_err(),
                "{key} should be rejected"
            );
        }
        assert!(validate_envelope(&json!([]), &p).is_err());
        assert!(validate_envelope(&json!(null), &p).is_err());
        let mut bad = params();
        bad.kdf.memory = 1 << 30;
        assert!(validate_envelope(&envelope(), &bad).is_err());
        bad = params();
        bad.salt = "nothex".into();
        assert!(validate_envelope(&envelope(), &bad).is_err());
        bad = params();
        bad.version = 0;
        assert!(validate_envelope(&envelope(), &bad).is_err());
    }

    #[test]
    fn kdf_serialises_like_the_browser_constant() {
        assert_eq!(
            serde_json::to_value(KDF).unwrap(),
            json!({"algorithm":"argon2id","version":19,"memory":65536,"iterations":3,"parallelism":4})
        );
    }

    #[test]
    fn email_normalisation() {
        assert_eq!(
            email(Some(&json!("  Alice@Example.COM "))).unwrap(),
            "alice@example.com"
        );
        for bad in [
            "alice",
            "alice@",
            "@example.com",
            "alice@example",
            "a b@example.com",
            "a@b@c.d",
            "",
        ] {
            assert!(email(Some(&json!(bad))).is_err(), "{bad:?}");
        }
        assert!(email(Some(&json!(5))).is_err());
        assert!(email(None).is_err());
        assert!(email(Some(&json!(format!("{}@example.com", "a".repeat(250))))).is_err());
    }

    #[test]
    fn token_requires_32_hex_bytes() {
        assert!(token(Some(&json!("ab".repeat(32)))).is_ok());
        assert!(token(Some(&json!("AB".repeat(32)))).is_err());
        assert!(token(Some(&json!("ab".repeat(31)))).is_err());
        assert!(token(None).is_err());
    }

    #[test]
    fn environment_pattern() {
        assert!(valid_environment("staging"));
        assert!(valid_environment("a-1"));
        assert!(!valid_environment(""));
        assert!(!valid_environment("Prod"));
        assert!(!valid_environment(&"a".repeat(41)));
    }
}
