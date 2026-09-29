//! Hosted-login TOTP. This does not alter the immutable Lit owner authorization
//! protocol. Enrollment secrets are encrypted with a domain-separated server key;
//! recovery codes and pending login tokens are stored only as hashes.
use crate::{
    api::{self, ApiError, ApiResult, PrivateJson},
    auth::{self, SameOrigin, Session},
    authority, billing,
    chipotle::Chipotle,
    config::Config,
    crypto,
    models::{field, number, Signed},
    registry, sponsorship,
};
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use rand::RngCore;
use rocket::{
    get,
    http::{CookieJar, Status},
    post,
    serde::json::Json,
    State,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::Sha256;
use sqlx::{PgPool, Postgres, Transaction};
use subtle::ConstantTimeEq;

type Tx<'a> = Transaction<'a, Postgres>;
const ISSUER: &str = "Lit Keychain";

fn encryption_key(root: &[u8; 32]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(root).expect("HMAC key");
    mac.update(b"lit-keychain/totp-encryption/v1");
    mac.finalize().into_bytes().into()
}
fn seal(secret: &str, vault: &str, cfg: &Config) -> Result<String, ApiError> {
    sponsorship::encrypt_key(
        secret,
        vault,
        &encryption_key(&cfg.usage_key_encryption_key),
    )
    .map_err(api::internal)
}
fn unseal(secret: &str, vault: &str, cfg: &Config) -> Result<Vec<u8>, ApiError> {
    let encoded = sponsorship::decrypt_key(
        secret,
        vault,
        &encryption_key(&cfg.usage_key_encryption_key),
    )
    .map_err(api::internal)?;
    BASE32_NOPAD
        .decode(encoded.as_bytes())
        .map_err(api::internal)
}
// RFC 6238 / RFC 4226, six digits, SHA-1 and a 30-second time step.
fn totp(secret: &[u8], step: i64, digits: u32) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("HMAC key");
    mac.update(&(step as u64).to_be_bytes());
    let bytes = mac.finalize().into_bytes();
    let offset = (bytes[19] & 0x0f) as usize;
    let value =
        u32::from_be_bytes(bytes[offset..offset + 4].try_into().expect("four bytes")) & 0x7fffffff;
    format!(
        "{:0width$}",
        value % 10u32.pow(digits),
        width = digits as usize
    )
}
fn matched_step(secret: &[u8], code: &str, now: i64, last: i64) -> Option<i64> {
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut matched = None;
    for step in [now / 30 - 1, now / 30, now / 30 + 1] {
        let equal = totp(secret, step, 6).as_bytes().ct_eq(code.as_bytes());
        if bool::from(equal) && step > last && step >= 0 {
            matched = Some(step);
        }
    }
    matched
}
fn recovery_hash(vault: &str, code: &str) -> Option<String> {
    if code.len() > 64 {
        return None;
    }
    let normalized: String = code
        .chars()
        .filter(|c| *c != '-' && *c != ' ')
        .collect::<String>()
        .to_ascii_lowercase();
    if normalized.len() != 32 || !normalized.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(crypto::hash_bytes(
        format!("lit-keychain/totp-recovery/v1:{vault}:{normalized}").as_bytes(),
    ))
}
fn invalid_code() -> ApiError {
    api::err(Status::Forbidden, "invalid_or_used_two_factor_code")
}
async fn attempts(pool: &PgPool, vault: &str) -> Result<(), ApiError> {
    // Persisted outside the verification transaction, including failed attempts.
    // All sessions and pending logins for a vault share this limit.
    billing::reserve(pool, &format!("totp:{vault}"), 300, 10).await
}
async fn audit(tx: &mut Tx<'_>, vault: &str, event: &str) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO kc_audit(vault_id,event) VALUES($1,$2)")
        .bind(vault)
        .bind(event)
        .execute(&mut **tx)
        .await
        .map_err(api::internal)?;
    Ok(())
}
pub(crate) async fn enabled(tx: &mut Tx<'_>, vault: &str) -> Result<bool, ApiError> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM kc_two_factor WHERE vault_id=$1)")
        .bind(vault)
        .fetch_one(&mut **tx)
        .await
        .map_err(api::internal)
}
async fn lock_session(tx: &mut Tx<'_>, session: &Session) -> Result<(), ApiError> {
    registry::lock_vault(tx, &session.vault_id).await?;
    let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM kc_sessions WHERE token_hash=$1 AND vault_id=$2 AND expires_at>now())")
        .bind(&session.token_hash).bind(&session.vault_id).fetch_one(&mut **tx).await.map_err(api::internal)?;
    if !valid {
        return Err(api::err(Status::Unauthorized, "session_expired"));
    }
    Ok(())
}
pub(crate) async fn invalidate_pending(tx: &mut Tx<'_>, vault: &str) -> Result<(), ApiError> {
    for query in [
        "DELETE FROM kc_two_factor_logins WHERE vault_id=$1",
        "DELETE FROM kc_two_factor_setup WHERE vault_id=$1",
        "DELETE FROM kc_challenges WHERE vault_id=$1",
    ] {
        sqlx::query(query)
            .bind(vault)
            .execute(&mut **tx)
            .await
            .map_err(api::internal)?;
    }
    Ok(())
}
async fn revoke_others(tx: &mut Tx<'_>, session: &Session) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM kc_sessions WHERE vault_id=$1 AND token_hash<>$2")
        .bind(&session.vault_id)
        .bind(&session.token_hash)
        .execute(&mut **tx)
        .await
        .map_err(api::internal)?;
    invalidate_pending(tx, &session.vault_id).await
}
async fn new_recovery_codes(tx: &mut Tx<'_>, vault: &str) -> Result<Vec<String>, ApiError> {
    sqlx::query("DELETE FROM kc_two_factor_recovery WHERE vault_id=$1")
        .bind(vault)
        .execute(&mut **tx)
        .await
        .map_err(api::internal)?;
    let mut codes = Vec::new();
    for _ in 0..10 {
        let mut bytes = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let raw = hex::encode(bytes);
        let code = format!(
            "{}-{}-{}-{}",
            &raw[..8],
            &raw[8..16],
            &raw[16..24],
            &raw[24..]
        );
        sqlx::query("INSERT INTO kc_two_factor_recovery(vault_id,code_hash) VALUES($1,$2)")
            .bind(vault)
            .bind(recovery_hash(vault, &code).expect("generated recovery code"))
            .execute(&mut **tx)
            .await
            .map_err(api::internal)?;
        codes.push(code);
    }
    Ok(codes)
}
/// Caller holds the vault lock. Replays and concurrent recovery attempts cannot
/// issue multiple sessions, and a failed transaction never consumes the code.
async fn verify_code(
    tx: &mut Tx<'_>,
    cfg: &Config,
    vault: &str,
    code: &str,
) -> Result<(), ApiError> {
    let (encrypted, last): (String, i64) =
        sqlx::query_as("SELECT encrypted_secret,last_step FROM kc_two_factor WHERE vault_id=$1")
            .bind(vault)
            .fetch_optional(&mut **tx)
            .await
            .map_err(api::internal)?
            .ok_or_else(invalid_code)?;
    let secret = unseal(&encrypted, vault, cfg)?;
    if let Some(step) = matched_step(
        &secret,
        code.trim(),
        time::OffsetDateTime::now_utc().unix_timestamp(),
        last,
    ) {
        sqlx::query("UPDATE kc_two_factor SET last_step=$2 WHERE vault_id=$1")
            .bind(vault)
            .bind(step)
            .execute(&mut **tx)
            .await
            .map_err(api::internal)?;
        return Ok(());
    }
    if let Some(hash) = recovery_hash(vault, code) {
        let used =
            sqlx::query("DELETE FROM kc_two_factor_recovery WHERE vault_id=$1 AND code_hash=$2")
                .bind(vault)
                .bind(hash)
                .execute(&mut **tx)
                .await
                .map_err(api::internal)?;
        if used.rows_affected() == 1 {
            audit(tx, vault, "two_factor_recovery_used").await?;
            return Ok(());
        }
    }
    Err(invalid_code())
}

#[get("/api/security")]
pub async fn status(session: Session, pool: &State<PgPool>) -> ApiResult<Value> {
    let row: (bool, i64) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM kc_two_factor WHERE vault_id=$1), (SELECT count(*) FROM kc_two_factor_recovery WHERE vault_id=$1)")
        .bind(&session.vault_id).fetch_one(pool.inner()).await.map_err(api::internal)?;
    Ok(Json(
        json!({"enabled":row.0,"recoveryCodesRemaining":row.1}),
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Setup,
    Disable,
    Regenerate,
}
impl Operation {
    fn purpose(&self) -> &'static str {
        match self {
            Self::Setup => "totp:setup",
            Self::Disable => "totp:disable",
            Self::Regenerate => "totp:regenerate",
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeRequest {
    operation: Operation,
}
#[post("/api/security/challenge", format = "json", data = "<body>")]
pub async fn challenge(
    _origin: SameOrigin,
    session: Session,
    body: PrivateJson<ChallengeRequest>,
    pool: &State<PgPool>,
) -> ApiResult<Value> {
    billing::reserve(
        pool,
        &format!("totp-challenge:{}", session.vault_id),
        300,
        20,
    )
    .await?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    lock_session(&mut tx, &session).await?;
    let nonce = crypto::random_token();
    let expires = time::OffsetDateTime::now_utc() + time::Duration::minutes(5);
    sqlx::query("INSERT INTO kc_challenges(challenge,vault_id,expires_at,purpose,session_hash) VALUES($1,$2,$3,$4,$5)")
        .bind(&nonce).bind(&session.vault_id).bind(expires).bind(body.operation.purpose()).bind(&session.token_hash)
        .execute(&mut *tx).await.map_err(api::internal)?;
    let cid: String = sqlx::query_scalar("SELECT authority_cid FROM kc_vaults WHERE id=$1")
        .bind(&session.vault_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(api::internal)?;
    tx.commit().await.map_err(api::internal)?;
    // Reuse the existing immutable owner-proof format. The opaque nonce is
    // bound in the DB to this session and exact operation, never to login.
    Ok(Json(
        json!({"v":2,"domain":"lit-keychain/v2","kind":"login","vaultId":session.vault_id,"challenge":nonce,"expiresAt":expires.unix_timestamp(),"authorityCid":cid}),
    ))
}
async fn verify_approval(
    pool: &PgPool,
    lit: &Chipotle,
    session: &Session,
    signed: &Signed,
) -> Result<(), ApiError> {
    authority::verify_any(pool, lit, &session.vault_id, signed).await?;
    if field(&signed.document, "kind").map_err(api::invalid)? != "login"
        || number(&signed.document, "expiresAt").map_err(api::invalid)?
            <= time::OffsetDateTime::now_utc().unix_timestamp()
    {
        return Err(api::err(Status::Forbidden, "approval_expired"));
    }
    Ok(())
}
async fn consume_approval(
    tx: &mut Tx<'_>,
    session: &Session,
    signed: &Signed,
    operation: Operation,
) -> Result<(), ApiError> {
    let used = sqlx::query("DELETE FROM kc_challenges WHERE challenge=$1 AND vault_id=$2 AND purpose=$3 AND session_hash=$4 AND expires_at>now()")
        .bind(field(&signed.document, "challenge").map_err(api::invalid)?).bind(&session.vault_id)
        .bind(operation.purpose()).bind(&session.token_hash).execute(&mut **tx).await.map_err(api::internal)?;
    if used.rows_affected() != 1 {
        return Err(api::err(Status::Forbidden, "approval_used_or_expired"));
    }
    Ok(())
}
#[post("/api/security/totp/setup", format = "json", data = "<body>")]
pub async fn setup(
    _origin: SameOrigin,
    session: Session,
    body: PrivateJson<Signed>,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    verify_approval(pool, lit, &session, &body).await?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    lock_session(&mut tx, &session).await?;
    consume_approval(&mut tx, &session, &body, Operation::Setup).await?;
    if enabled(&mut tx, &session.vault_id).await? {
        return Err(api::err(Status::Conflict, "two_factor_already_enabled"));
    }
    let mut bytes = [0u8; 20];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let secret = BASE32_NOPAD.encode(&bytes);
    sqlx::query("INSERT INTO kc_two_factor_setup(vault_id,session_hash,encrypted_secret,expires_at) VALUES($1,$2,$3,now()+interval '10 minutes') ON CONFLICT(vault_id) DO UPDATE SET session_hash=$2,encrypted_secret=$3,expires_at=now()+interval '10 minutes'")
        .bind(&session.vault_id).bind(&session.token_hash).bind(seal(&secret, &session.vault_id, cfg)?)
        .execute(&mut *tx).await.map_err(api::internal)?;
    audit(&mut tx, &session.vault_id, "two_factor_setup_started").await?;
    tx.commit().await.map_err(api::internal)?;
    let mut uri = reqwest::Url::parse(&format!(
        "otpauth://totp/Lit%20Keychain:{}",
        session.vault_id
    ))
    .map_err(api::internal)?;
    uri.query_pairs_mut()
        .append_pair("secret", &secret)
        .append_pair("issuer", ISSUER)
        .append_pair("algorithm", "SHA1")
        .append_pair("digits", "6")
        .append_pair("period", "30");
    Ok(Json(json!({"secret":secret,"uri":uri.as_str()})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Code {
    code: String,
}
#[post("/api/security/totp/confirm", format = "json", data = "<body>")]
pub async fn confirm(
    _origin: SameOrigin,
    session: Session,
    body: PrivateJson<Code>,
    pool: &State<PgPool>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    attempts(pool, &session.vault_id).await?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    lock_session(&mut tx, &session).await?;
    if enabled(&mut tx, &session.vault_id).await? {
        return Err(api::err(Status::Conflict, "two_factor_already_enabled"));
    }
    let encrypted: String = sqlx::query_scalar("SELECT encrypted_secret FROM kc_two_factor_setup WHERE vault_id=$1 AND session_hash=$2 AND expires_at>now()")
        .bind(&session.vault_id).bind(&session.token_hash).fetch_optional(&mut *tx).await.map_err(api::internal)?
        .ok_or_else(|| api::err(Status::Gone, "two_factor_setup_expired"))?;
    let secret = unseal(&encrypted, &session.vault_id, cfg)?;
    let step = matched_step(
        &secret,
        body.code.trim(),
        time::OffsetDateTime::now_utc().unix_timestamp(),
        -1,
    )
    .ok_or_else(invalid_code)?;
    sqlx::query("INSERT INTO kc_two_factor(vault_id,encrypted_secret,last_step) VALUES($1,$2,$3)")
        .bind(&session.vault_id)
        .bind(encrypted)
        .bind(step)
        .execute(&mut *tx)
        .await
        .map_err(api::internal)?;
    let codes = new_recovery_codes(&mut tx, &session.vault_id).await?;
    revoke_others(&mut tx, &session).await?;
    audit(&mut tx, &session.vault_id, "two_factor_enabled").await?;
    tx.commit().await.map_err(api::internal)?;
    Ok(Json(json!({"recoveryCodes":codes})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    authorization: Signed,
    code: String,
}
async fn change(
    session: Session,
    body: Change,
    pool: &PgPool,
    lit: &Chipotle,
    cfg: &Config,
    operation: Operation,
) -> ApiResult<Value> {
    attempts(pool, &session.vault_id).await?;
    verify_approval(pool, lit, &session, &body.authorization).await?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    lock_session(&mut tx, &session).await?;
    let removing = matches!(operation, Operation::Disable);
    consume_approval(&mut tx, &session, &body.authorization, operation).await?;
    verify_code(&mut tx, cfg, &session.vault_id, &body.code).await?;
    let result = if removing {
        sqlx::query("DELETE FROM kc_two_factor WHERE vault_id=$1")
            .bind(&session.vault_id)
            .execute(&mut *tx)
            .await
            .map_err(api::internal)?;
        sqlx::query("DELETE FROM kc_two_factor_recovery WHERE vault_id=$1")
            .bind(&session.vault_id)
            .execute(&mut *tx)
            .await
            .map_err(api::internal)?;
        audit(&mut tx, &session.vault_id, "two_factor_disabled").await?;
        json!({"enabled":false})
    } else {
        let codes = new_recovery_codes(&mut tx, &session.vault_id).await?;
        audit(
            &mut tx,
            &session.vault_id,
            "two_factor_recovery_regenerated",
        )
        .await?;
        json!({"recoveryCodes":codes})
    };
    revoke_others(&mut tx, &session).await?;
    tx.commit().await.map_err(api::internal)?;
    Ok(Json(result))
}
#[post("/api/security/totp/disable", format = "json", data = "<body>")]
pub async fn disable(
    _origin: SameOrigin,
    session: Session,
    body: PrivateJson<Change>,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    change(
        session,
        body.into_inner(),
        pool,
        lit,
        cfg,
        Operation::Disable,
    )
    .await
}
#[post("/api/security/totp/regenerate", format = "json", data = "<body>")]
pub async fn regenerate(
    _origin: SameOrigin,
    session: Session,
    body: PrivateJson<Change>,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    change(
        session,
        body.into_inner(),
        pool,
        lit,
        cfg,
        Operation::Regenerate,
    )
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyLogin {
    token: String,
    code: String,
}
#[post("/auth/two-factor", format = "json", data = "<body>")]
pub async fn login(
    _origin: SameOrigin,
    peer: billing::Peer,
    body: PrivateJson<VerifyLogin>,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    cookies: &CookieJar<'_>,
) -> ApiResult<Value> {
    billing::reserve(pool, &format!("totp-ip:{}", peer.0), 300, 50).await?;
    if !crate::models::valid_hex(&body.token, 32) {
        return Err(invalid_code());
    }
    let hash = crypto::hash_bytes(body.token.as_bytes());
    let vault: String = sqlx::query_scalar(
        "SELECT vault_id FROM kc_two_factor_logins WHERE token_hash=$1 AND expires_at>now()",
    )
    .bind(&hash)
    .fetch_optional(pool.inner())
    .await
    .map_err(api::internal)?
    .ok_or_else(|| api::err(Status::Gone, "two_factor_login_expired"))?;
    attempts(pool, &vault).await?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    registry::lock_vault(&mut tx, &vault).await?;
    let used =
        sqlx::query("DELETE FROM kc_two_factor_logins WHERE token_hash=$1 AND expires_at>now()")
            .bind(&hash)
            .execute(&mut *tx)
            .await
            .map_err(api::internal)?;
    if used.rows_affected() != 1 {
        return Err(api::err(Status::Gone, "two_factor_login_expired"));
    }
    verify_code(&mut tx, cfg, &vault, &body.code).await?;
    let token = auth::issue_session(&mut tx, &vault).await?;
    tx.commit().await.map_err(api::internal)?;
    auth::set_cookie(cookies, cfg, token);
    Ok(Json(json!({"vaultId":vault})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rfc6238_sha1_vectors_and_replay_window() {
        let secret = b"12345678901234567890";
        for (time, expected) in [
            (59, "94287082"),
            (1111111109, "07081804"),
            (1111111111, "14050471"),
            (1234567890, "89005924"),
            (2000000000, "69279037"),
            (20000000000, "65353130"),
        ] {
            assert_eq!(totp(secret, time / 30, 8), expected);
        }
        assert_eq!(matched_step(secret, "287082", 59, -1), Some(1));
        assert_eq!(matched_step(secret, "287082", 60, -1), Some(1));
        assert_eq!(matched_step(secret, "287082", 29, -1), Some(1));
        assert_eq!(matched_step(secret, "287082", 90, -1), None);
        assert_eq!(matched_step(secret, "287082", 59, 1), None);
        assert_eq!(matched_step(secret, "2870820", 59, -1), None);
    }
    #[test]
    fn enrollment_cipher_is_separate_and_vault_bound() {
        let root = [47u8; 32];
        let key = encryption_key(&root);
        assert_ne!(key, root);
        let sealed = sponsorship::encrypt_key("fixture-secret", "vault-a", &key).unwrap();
        assert_eq!(
            sponsorship::decrypt_key(&sealed, "vault-a", &key).unwrap(),
            "fixture-secret"
        );
        assert!(sponsorship::decrypt_key(&sealed, "vault-b", &key).is_err());
        assert!(sponsorship::decrypt_key(&sealed, "vault-a", &root).is_err());
    }
    #[test]
    fn recovery_codes_are_normalized_and_vault_bound() {
        let raw = "12345678abcdefab12345678abcdefab";
        assert_eq!(
            recovery_hash("vault", raw),
            recovery_hash("vault", "12345678-ABCDEFAB-12345678-ABCDEFAB")
        );
        assert_ne!(recovery_hash("vault", raw), recovery_hash("other", raw));
        assert_eq!(recovery_hash("vault", "123456"), None);
    }
}
