//! Admin console API (plans/tee-chat-app.md section 4.4). Zero user-data
//! routes: nothing here can read conversations, messages, or user KEKs, and
//! the deployment's DB role has no grants on those tables either.

use crate::admin::{
    admin_cookie, admin_csrf, issue_admin, AdminClaims, AdminCsrf, AdminStage, AdminUser,
    PendingAdmin, WebauthnState, ADMIN_IDLE_TTL_SECS, PRE2FA_TTL_SECS,
};
use crate::config::Config;
use crate::crypto::{constant_time_eq, hmac_sha256};
use crate::identity;
use crate::mail::{code_email, Mailer};
use crate::metering;
use crate::openrouter::OpenRouterClient;
use crate::rate_limit::StreamRateLimit;
use crate::routes::{err, ApiError};
use crate::session::{self, ADMIN_SESSION_COOKIE};
use crate::store::{admin as admin_store, meters, provider_keys, settings, users};
use crate::Keyring;
use base64::Engine;
use rand::Rng;
use rocket::http::{Cookie, CookieJar, SameSite, Status};
use rocket::serde::json::Json;
use rocket::{delete, get, post, State};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use webauthn_rs::prelude::{
    Passkey, PasskeyAuthentication, PasskeyRegistration, PublicKeyCredential,
    RegisterPublicKeyCredential,
};

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

const ADMIN_PENDING_COOKIE: &str = "__Host-lit_chat_admin_pending";
const MAGIC_LINK_TTL_SECS: i64 = 15 * 60;
const CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const CODE_LEN: usize = 8;

fn internal(e: impl std::fmt::Display, what: &str) -> ApiError {
    tracing::warn!("{what}: {e}");
    err(Status::InternalServerError, "server_error")
}

fn hash_b64(data: &[u8]) -> String {
    use sha2::Digest;
    B64.encode(sha2::Sha256::digest(data))
}

fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    (0..CODE_LEN)
        .map(|_| CODE_ALPHABET[rng.gen_range(0..CODE_ALPHABET.len())] as char)
        .collect()
}

// ---------------------------------------------------------------------------
// Admin magic-link (scope-prefixed pending token so consumer and admin
// pending tokens are structurally distinct even before the MAC check).

struct AdminPending {
    email: String,
    nonce: String,
    code_hash: String,
}

fn issue_admin_pending(
    mac_key: &[u8; 32],
    email: &str,
    expires_unix: i64,
    nonce: &str,
    code_hash: &str,
) -> String {
    let payload = format!("admin|{email}|{expires_unix}|{nonce}|{code_hash}");
    let sig = hmac_sha256(mac_key, payload.as_bytes());
    format!("{}.{}", B64.encode(payload.as_bytes()), B64.encode(sig))
}

fn verify_admin_pending(mac_key: &[u8; 32], token: &str, now_unix: i64) -> Option<AdminPending> {
    let (payload_b64, sig_b64) = token.split_once('.')?;
    let payload_bytes = B64.decode(payload_b64).ok()?;
    let provided = B64.decode(sig_b64).ok()?;
    if !constant_time_eq(&provided, &hmac_sha256(mac_key, &payload_bytes)) {
        return None;
    }
    let payload = std::str::from_utf8(&payload_bytes).ok()?;
    let parts: Vec<&str> = payload.split('|').collect();
    if parts.len() != 5 || parts[0] != "admin" {
        return None;
    }
    let expires_unix: i64 = parts[2].parse().ok()?;
    if expires_unix <= now_unix {
        return None;
    }
    Some(AdminPending {
        email: parts[1].to_string(),
        nonce: parts[3].to_string(),
        code_hash: parts[4].to_string(),
    })
}

#[derive(Debug, Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

#[derive(Debug, Deserialize)]
pub struct RequestCodeBody {
    pub email: String,
}

#[post("/admin/api/auth/request", format = "json", data = "<body>")]
pub async fn auth_request(
    _rate: StreamRateLimit,
    body: Json<RequestCodeBody>,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    keyring: &State<Keyring>,
    mailer: &State<Option<Mailer>>,
    cookies: &CookieJar<'_>,
) -> Json<OkResponse> {
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') || email.len() > 254 {
        return Json(OkResponse { ok: true });
    }
    // No roster check here: responding identically for non-admins avoids
    // roster enumeration. Verify rejects non-admins after code entry.
    let code = generate_code();
    let nonce = hex::encode(rand::thread_rng().gen::<[u8; 16]>());
    let expires_unix = session::now_unix() + MAGIC_LINK_TTL_SECS;
    let token = issue_admin_pending(
        &keyring.magic_link_mac,
        &email,
        expires_unix,
        &nonce,
        &hash_b64(code.as_bytes()),
    );
    let expires_at = time::OffsetDateTime::from_unix_timestamp(expires_unix)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    if let Err(e) = users::store_magic_link(pool, &hash_b64(nonce.as_bytes()), expires_at).await {
        tracing::warn!("admin magic-link store failed: {e}");
        return Json(OkResponse { ok: true });
    }
    cookies.add(
        Cookie::build((ADMIN_PENDING_COOKIE, token))
            .http_only(true)
            .secure(true)
            .same_site(SameSite::Lax)
            .path("/")
            .max_age(rocket::time::Duration::seconds(MAGIC_LINK_TTL_SECS))
            .build(),
    );
    if cfg.dev_echo_codes {
        tracing::info!("dev admin sign-in code: {code}");
    } else if let Some(mailer) = mailer.inner() {
        let (subject, html, text) = code_email(&code);
        let mailer = mailer.clone();
        tokio::spawn(async move {
            if let Err(e) = mailer.send(&email, &subject, &html, &text).await {
                tracing::warn!("admin magic-link send failed: {e}");
            }
        });
    }
    Json(OkResponse { ok: true })
}

#[derive(Debug, Deserialize)]
pub struct VerifyCodeBody {
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub ok: bool,
    /// "register_passkey" (first login) or "passkey_required".
    pub next: &'static str,
}

#[post("/admin/api/auth/verify", format = "json", data = "<body>")]
pub async fn auth_verify(
    _rate: StreamRateLimit,
    body: Json<VerifyCodeBody>,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
    cookies: &CookieJar<'_>,
) -> Result<Json<VerifyResponse>, ApiError> {
    let Some(pending_cookie) = cookies.get(ADMIN_PENDING_COOKIE) else {
        return Err(err(Status::BadRequest, "no_pending_login"));
    };
    let Some(pending) = verify_admin_pending(
        &keyring.magic_link_mac,
        pending_cookie.value(),
        session::now_unix(),
    ) else {
        return Err(err(Status::BadRequest, "invalid_or_expired"));
    };
    let code = body.code.trim().to_uppercase();
    if !constant_time_eq(
        hash_b64(code.as_bytes()).as_bytes(),
        pending.code_hash.as_bytes(),
    ) {
        return Err(err(Status::BadRequest, "invalid_code"));
    }
    let consumed = users::consume_magic_link(pool, &hash_b64(pending.nonce.as_bytes()))
        .await
        .map_err(|e| internal(e, "admin magic-link consume"))?;
    if !consumed {
        return Err(err(Status::BadRequest, "invalid_or_expired"));
    }

    let user_ref = identity::account_user_ref(&keyring.user_id_namespace, &pending.email);
    let hash = identity::user_ref_hash(&user_ref);
    let is_admin = admin_store::is_admin(pool, &keyring.admin_roster_mac, &hash)
        .await
        .map_err(|e| internal(e, "roster check"))?;
    if !is_admin {
        return Err(err(Status::Forbidden, "not_an_admin"));
    }

    let now = session::now_unix();
    let claims = AdminClaims {
        user_ref,
        stage: AdminStage::Pre2fa,
        issued_unix: now,
        expires_unix: now + PRE2FA_TTL_SECS,
        sid: session::generate_sid(),
    };
    let token = issue_admin(&keyring.admin_session_mac, &claims);
    cookies.add(admin_cookie(token, PRE2FA_TTL_SECS));
    cookies.remove(Cookie::build((ADMIN_PENDING_COOKIE, "")).path("/").build());

    let has_passkey = admin_store::credential_count(pool, &hash)
        .await
        .map_err(|e| internal(e, "credential count"))?
        > 0;
    Ok(Json(VerifyResponse {
        ok: true,
        next: if has_passkey {
            "passkey_required"
        } else {
            "register_passkey"
        },
    }))
}

#[post("/admin/api/auth/logout")]
pub async fn logout(
    admin: AdminUser,
    _csrf: AdminCsrf,
    pool: &State<PgPool>,
    cookies: &CookieJar<'_>,
) -> Json<OkResponse> {
    if let Err(e) = users::revoke_session(pool, &admin.token_hash).await {
        tracing::warn!("admin session revoke failed: {e}");
    }
    cookies.remove(Cookie::build((ADMIN_SESSION_COOKIE, "")).path("/").build());
    Json(OkResponse { ok: true })
}

#[get("/admin/api/me")]
pub async fn me(admin: AdminUser, keyring: &State<Keyring>) -> Json<serde_json::Value> {
    Json(json!({
        "user_ref_hash": admin.user_ref_hash,
        "csrf": admin_csrf(&keyring.admin_session_mac, &admin.sid),
        "stage": "full",
    }))
}

// ---------------------------------------------------------------------------
// WebAuthn: mandatory second factor, registered at first admin login.

fn webauthn_user_uuid(user_ref_hash: &str) -> Uuid {
    let bytes = hex::decode(user_ref_hash).unwrap_or_default();
    if bytes.len() >= 16 {
        Uuid::from_slice(&bytes[..16]).unwrap_or_else(|_| Uuid::nil())
    } else {
        Uuid::nil()
    }
}

#[post("/admin/api/webauthn/register/start")]
pub async fn webauthn_register_start(
    pending: PendingAdmin,
    wa: &State<WebauthnState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = webauthn_user_uuid(&pending.user_ref_hash);
    let label = format!("admin-{}", &pending.user_ref_hash[..8]);
    let (challenge, reg_state) = wa
        .webauthn
        .start_passkey_registration(uuid, &label, &label, None)
        .map_err(|e| internal(e, "webauthn register start"))?;
    let state_json =
        serde_json::to_string(&reg_state).map_err(|e| internal(e, "webauthn state serialize"))?;
    wa.put_reg(&pending.sid, state_json).await;
    Ok(Json(
        serde_json::to_value(challenge).map_err(|e| internal(e, "challenge"))?,
    ))
}

#[post(
    "/admin/api/webauthn/register/finish",
    format = "json",
    data = "<body>"
)]
pub async fn webauthn_register_finish(
    pending: PendingAdmin,
    body: Json<RegisterPublicKeyCredential>,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
    wa: &State<WebauthnState>,
    cookies: &CookieJar<'_>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Some(state_json) = wa.take_reg(&pending.sid).await else {
        return Err(err(Status::BadRequest, "no_registration_in_flight"));
    };
    let reg_state: PasskeyRegistration =
        serde_json::from_str(&state_json).map_err(|e| internal(e, "webauthn state parse"))?;
    let passkey = wa
        .webauthn
        .finish_passkey_registration(&body, &reg_state)
        .map_err(|_| err(Status::BadRequest, "passkey_rejected"))?;
    let cred_id = B64.encode(passkey.cred_id().as_ref());
    let passkey_json =
        serde_json::to_vec(&passkey).map_err(|e| internal(e, "passkey serialize"))?;
    admin_store::store_credential(
        pool,
        &keyring.cred_kek,
        &cred_id,
        &pending.user_ref_hash,
        &passkey_json,
    )
    .await
    .map_err(|e| internal(e, "passkey store"))?;
    admin_store::audit(
        pool,
        &keyring.audit_mac,
        &pending.user_ref_hash,
        "passkey.register",
        &cred_id,
        "{}",
    )
    .await
    .map_err(|e| internal(e, "audit"))?;

    let (token, csrf) = full_session(keyring, &pending.user_ref);
    cookies.add(admin_cookie(token, ADMIN_IDLE_TTL_SECS));
    Ok(Json(json!({"ok": true, "csrf": csrf})))
}

#[post("/admin/api/webauthn/auth/start")]
pub async fn webauthn_auth_start(
    pending: PendingAdmin,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
    wa: &State<WebauthnState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let blobs = admin_store::load_credentials(pool, &keyring.cred_kek, &pending.user_ref_hash)
        .await
        .map_err(|e| internal(e, "load credentials"))?;
    let mut passkeys: Vec<Passkey> = Vec::with_capacity(blobs.len());
    for blob in blobs {
        passkeys.push(serde_json::from_slice(&blob).map_err(|e| internal(e, "passkey parse"))?);
    }
    if passkeys.is_empty() {
        return Err(err(Status::BadRequest, "no_passkeys"));
    }
    let (challenge, auth_state) = wa
        .webauthn
        .start_passkey_authentication(&passkeys)
        .map_err(|e| internal(e, "webauthn auth start"))?;
    let state_json =
        serde_json::to_string(&auth_state).map_err(|e| internal(e, "webauthn state serialize"))?;
    wa.put_auth(&pending.sid, state_json).await;
    Ok(Json(
        serde_json::to_value(challenge).map_err(|e| internal(e, "challenge"))?,
    ))
}

#[post("/admin/api/webauthn/auth/finish", format = "json", data = "<body>")]
pub async fn webauthn_auth_finish(
    pending: PendingAdmin,
    body: Json<PublicKeyCredential>,
    keyring: &State<Keyring>,
    wa: &State<WebauthnState>,
    cookies: &CookieJar<'_>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Some(state_json) = wa.take_auth(&pending.sid).await else {
        return Err(err(Status::BadRequest, "no_auth_in_flight"));
    };
    let auth_state: PasskeyAuthentication =
        serde_json::from_str(&state_json).map_err(|e| internal(e, "webauthn state parse"))?;
    wa.webauthn
        .finish_passkey_authentication(&body, &auth_state)
        .map_err(|_| err(Status::Unauthorized, "passkey_rejected"))?;
    let (token, csrf) = full_session(keyring, &pending.user_ref);
    cookies.add(admin_cookie(token, ADMIN_IDLE_TTL_SECS));
    Ok(Json(json!({"ok": true, "csrf": csrf})))
}

fn full_session(keyring: &Keyring, user_ref: &str) -> (String, String) {
    let now = session::now_unix();
    let sid = session::generate_sid();
    let claims = AdminClaims {
        user_ref: user_ref.to_string(),
        stage: AdminStage::Full,
        issued_unix: now,
        expires_unix: now + ADMIN_IDLE_TTL_SECS,
        sid: sid.clone(),
    };
    let token = issue_admin(&keyring.admin_session_mac, &claims);
    let csrf = admin_csrf(&keyring.admin_session_mac, &sid);
    (token, csrf)
}

// ---------------------------------------------------------------------------
// Provider keys: write-only custody, masked display everywhere, no reveal
// endpoint. Rotation drives OpenRouter's provisioning API.

#[derive(Debug, Serialize)]
pub struct KeyInfo {
    pub id: Uuid,
    pub provider: String,
    pub kind: String,
    pub masked_hint: String,
    pub status: String,
    pub spend_limit_usd: Option<f64>,
    pub created_by: String,
    pub created_at: String,
}

fn key_info(row: &provider_keys::ProviderKeyRow) -> KeyInfo {
    KeyInfo {
        id: row.id,
        provider: row.provider.clone(),
        kind: row.kind.clone(),
        masked_hint: row.masked_hint.clone(),
        status: row.status.clone(),
        spend_limit_usd: row.spend_limit_usd,
        created_by: row.created_by.clone(),
        created_at: row
            .created_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
    }
}

#[get("/admin/api/keys")]
pub async fn list_keys(
    _admin: AdminUser,
    pool: &State<PgPool>,
) -> Result<Json<Vec<KeyInfo>>, ApiError> {
    let rows = provider_keys::list(pool, "openrouter")
        .await
        .map_err(|e| internal(e, "list keys"))?;
    Ok(Json(rows.iter().map(key_info).collect()))
}

#[derive(Debug, Deserialize)]
pub struct ImportKeyBody {
    /// Plaintext crosses the admin boundary exactly once, here.
    pub key: String,
    pub kind: String,
    pub spend_limit_usd: Option<f64>,
}

#[post("/admin/api/keys/import", format = "json", data = "<body>")]
pub async fn import_key(
    admin: AdminUser,
    _csrf: AdminCsrf,
    body: Json<ImportKeyBody>,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
) -> Result<Json<KeyInfo>, ApiError> {
    if body.kind != "runtime" && body.kind != "provisioning" {
        return Err(err(Status::BadRequest, "bad_kind"));
    }
    if body.key.len() < 16 || body.key.len() > 512 {
        return Err(err(Status::BadRequest, "bad_key"));
    }
    // Imported runtime keys start as standby (explicit promote); a new
    // provisioning key becomes active immediately and retires the old one.
    let status = if body.kind == "provisioning" {
        "active"
    } else {
        "standby"
    };
    if body.kind == "provisioning" {
        if let Some(old) = provider_keys::provisioning(pool, "openrouter")
            .await
            .map_err(|e| internal(e, "old provisioning lookup"))?
        {
            provider_keys::set_status(pool, old.id, "disabled")
                .await
                .map_err(|e| internal(e, "disable old provisioning"))?;
        }
    }
    let row = provider_keys::insert(
        pool,
        &keyring.provider_kek,
        "openrouter",
        &body.kind,
        &body.key,
        status,
        body.spend_limit_usd,
        None,
        &admin.user_ref_hash,
    )
    .await
    .map_err(|e| internal(e, "insert key"))?;
    admin_store::audit(
        pool,
        &keyring.audit_mac,
        &admin.user_ref_hash,
        "key.import",
        &row.id.to_string(),
        &json!({"kind": body.kind, "hint": row.masked_hint}).to_string(),
    )
    .await
    .map_err(|e| internal(e, "audit"))?;
    Ok(Json(key_info(&row)))
}

#[derive(Debug, Deserialize)]
pub struct MintKeyBody {
    pub name: String,
    pub spend_limit_usd: Option<f64>,
}

/// Mint a runtime key via OpenRouter's provisioning API: the key plaintext is
/// returned by OpenRouter directly to the enclave — the browser never sees it.
#[post("/admin/api/keys/mint", format = "json", data = "<body>")]
pub async fn mint_key(
    admin: AdminUser,
    _csrf: AdminCsrf,
    body: Json<MintKeyBody>,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
    or_client: &State<OpenRouterClient>,
) -> Result<Json<KeyInfo>, ApiError> {
    let Some(prov_row) = provider_keys::provisioning(pool, "openrouter")
        .await
        .map_err(|e| internal(e, "provisioning lookup"))?
    else {
        return Err(err(Status::Conflict, "no_provisioning_key"));
    };
    let prov_key = provider_keys::decrypt_key(&keyring.provider_kek, &prov_row)
        .map_err(|e| internal(e, "provisioning decrypt"))?;
    let minted = or_client
        .create_runtime_key(&prov_key, &body.name, body.spend_limit_usd)
        .await
        .map_err(|e| internal(e, "openrouter mint"))?;
    let row = provider_keys::insert(
        pool,
        &keyring.provider_kek,
        "openrouter",
        "runtime",
        &minted.key,
        "standby",
        body.spend_limit_usd,
        Some(&minted.hash),
        &admin.user_ref_hash,
    )
    .await
    .map_err(|e| internal(e, "insert minted key"))?;
    admin_store::audit(
        pool,
        &keyring.audit_mac,
        &admin.user_ref_hash,
        "key.mint",
        &row.id.to_string(),
        &json!({"hint": row.masked_hint, "limit": body.spend_limit_usd}).to_string(),
    )
    .await
    .map_err(|e| internal(e, "audit"))?;
    Ok(Json(key_info(&row)))
}

/// Promote to active; the previous active key demotes to `retiring` (grace
/// window so in-flight streams finish on the consumer's cached key).
#[post("/admin/api/keys/<id>/promote")]
pub async fn promote_key(
    admin: AdminUser,
    _csrf: AdminCsrf,
    id: Uuid,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Some(row) = provider_keys::get(pool, id)
        .await
        .map_err(|e| internal(e, "get key"))?
    else {
        return Err(err(Status::NotFound, "not_found"));
    };
    if row.kind != "runtime" {
        return Err(err(Status::BadRequest, "not_a_runtime_key"));
    }
    if row.status == "disabled" {
        return Err(err(Status::BadRequest, "key_disabled"));
    }
    provider_keys::demote_active_runtime(pool, "openrouter", id)
        .await
        .map_err(|e| internal(e, "demote"))?;
    provider_keys::set_status(pool, id, "active")
        .await
        .map_err(|e| internal(e, "promote"))?;
    admin_store::audit(
        pool,
        &keyring.audit_mac,
        &admin.user_ref_hash,
        "key.promote",
        &id.to_string(),
        &json!({"hint": row.masked_hint}).to_string(),
    )
    .await
    .map_err(|e| internal(e, "audit"))?;
    Ok(Json(json!({"ok": true})))
}

#[post("/admin/api/keys/<id>/retire")]
pub async fn retire_key(
    admin: AdminUser,
    _csrf: AdminCsrf,
    id: Uuid,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Some(row) = provider_keys::get(pool, id)
        .await
        .map_err(|e| internal(e, "get key"))?
    else {
        return Err(err(Status::NotFound, "not_found"));
    };
    provider_keys::set_status(pool, id, "retiring")
        .await
        .map_err(|e| internal(e, "retire"))?;
    admin_store::audit(
        pool,
        &keyring.audit_mac,
        &admin.user_ref_hash,
        "key.retire",
        &id.to_string(),
        &json!({"hint": row.masked_hint}).to_string(),
    )
    .await
    .map_err(|e| internal(e, "audit"))?;
    Ok(Json(json!({"ok": true})))
}

/// Disable: delete upstream first (best effort, via the provisioning API),
/// then mark disabled locally.
#[post("/admin/api/keys/<id>/disable")]
pub async fn disable_key(
    admin: AdminUser,
    _csrf: AdminCsrf,
    id: Uuid,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
    or_client: &State<OpenRouterClient>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Some(row) = provider_keys::get(pool, id)
        .await
        .map_err(|e| internal(e, "get key"))?
    else {
        return Err(err(Status::NotFound, "not_found"));
    };
    let mut upstream_deleted = false;
    if row.kind == "runtime" {
        if let Some(hash) = &row.upstream_hash {
            if let Ok(Some(prov_row)) = provider_keys::provisioning(pool, "openrouter").await {
                if let Ok(prov_key) = provider_keys::decrypt_key(&keyring.provider_kek, &prov_row) {
                    match or_client.delete_runtime_key(&prov_key, hash).await {
                        Ok(()) => upstream_deleted = true,
                        Err(e) => tracing::warn!("upstream key delete failed: {e}"),
                    }
                }
            }
        }
    }
    provider_keys::set_status(pool, id, "disabled")
        .await
        .map_err(|e| internal(e, "disable"))?;
    admin_store::audit(
        pool,
        &keyring.audit_mac,
        &admin.user_ref_hash,
        "key.disable",
        &id.to_string(),
        &json!({"hint": row.masked_hint, "upstream_deleted": upstream_deleted}).to_string(),
    )
    .await
    .map_err(|e| internal(e, "audit"))?;
    Ok(Json(
        json!({"ok": true, "upstream_deleted": upstream_deleted}),
    ))
}

/// Cheapest-model ping health probe for a stored key.
#[post("/admin/api/keys/<id>/probe")]
pub async fn probe_key(
    _admin: AdminUser,
    _csrf: AdminCsrf,
    id: Uuid,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
    or_client: &State<OpenRouterClient>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Some(row) = provider_keys::get(pool, id)
        .await
        .map_err(|e| internal(e, "get key"))?
    else {
        return Err(err(Status::NotFound, "not_found"));
    };
    if row.kind != "runtime" {
        return Err(err(Status::BadRequest, "not_a_runtime_key"));
    }
    let key = provider_keys::decrypt_key(&keyring.provider_kek, &row)
        .map_err(|e| internal(e, "key decrypt"))?;
    let model: Option<(String,)> = sqlx::query_as(
        "SELECT model_id FROM model_catalog WHERE enabled
         ORDER BY completion_usd_per_mtok ASC NULLS LAST LIMIT 1",
    )
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| internal(e, "cheapest model"))?;
    let Some((model_id,)) = model else {
        return Err(err(Status::Conflict, "no_enabled_models"));
    };
    match or_client.probe_key(&key, &model_id).await {
        Ok(()) => Ok(Json(json!({"ok": true, "model": model_id}))),
        Err(e) => {
            tracing::warn!(key_id = %id, "key probe failed: {e}");
            Ok(Json(json!({"ok": false, "model": model_id})))
        }
    }
}

// ---------------------------------------------------------------------------
// Spend / breaker / status

#[get("/admin/api/status")]
pub async fn status(
    _admin: AdminUser,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    keyring: &State<Keyring>,
    or_client: &State<OpenRouterClient>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mode = settings::breaker_mode(pool)
        .await
        .map_err(|e| internal(e, "breaker"))?;
    let defaults = settings::Caps {
        daily_spend_cap_micro_usd: cfg.daily_spend_cap_micro_usd,
        anon_daily_token_budget: cfg.anon_daily_token_budget,
    };
    let caps = settings::caps(pool, &defaults)
        .await
        .map_err(|e| internal(e, "caps"))?;
    let days = meters::recent_days(pool, 14)
        .await
        .map_err(|e| internal(e, "recent days"))?;
    let today = metering::today_utc();
    let spent_today = meters::day_spend(pool, &today)
        .await
        .map_err(|e| internal(e, "day spend"))?;

    // Credit balance via the provisioning key (low-balance alerting input).
    let mut credits = None;
    if let Ok(Some(prov_row)) = provider_keys::provisioning(pool, "openrouter").await {
        if let Ok(prov_key) = provider_keys::decrypt_key(&keyring.provider_kek, &prov_row) {
            match or_client.credits(&prov_key).await {
                Ok(c) => {
                    credits = Some(json!({
                        "total_credits": c.total_credits,
                        "total_usage": c.total_usage,
                    }))
                }
                Err(e) => tracing::warn!("credits lookup failed: {e}"),
            }
        }
    }

    let active = provider_keys::active_runtime(pool, "openrouter")
        .await
        .map_err(|e| internal(e, "active key"))?;
    Ok(Json(json!({
        "breaker_mode": mode,
        "caps": caps,
        "spend_today_micro_usd": spent_today,
        "recent_days": days.iter().map(|(d, m, r)| json!({
            "day": d, "micro_usd": m, "requests": r
        })).collect::<Vec<_>>(),
        "credits": credits,
        "active_key_hint": active.map(|r| r.masked_hint),
    })))
}

#[derive(Debug, Deserialize)]
pub struct BreakerBody {
    pub mode: settings::BreakerMode,
}

#[post("/admin/api/breaker", format = "json", data = "<body>")]
pub async fn set_breaker(
    admin: AdminUser,
    _csrf: AdminCsrf,
    body: Json<BreakerBody>,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let value = serde_json::to_string(&body.mode).map_err(|e| internal(e, "serialize"))?;
    settings::put(pool, settings::BREAKER_KEY, &value)
        .await
        .map_err(|e| internal(e, "put breaker"))?;
    admin_store::audit(
        pool,
        &keyring.audit_mac,
        &admin.user_ref_hash,
        "breaker.set",
        "breaker",
        &value,
    )
    .await
    .map_err(|e| internal(e, "audit"))?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Debug, Deserialize)]
pub struct CapsBody {
    pub daily_spend_cap_micro_usd: i64,
    pub anon_daily_token_budget: i64,
}

#[post("/admin/api/caps", format = "json", data = "<body>")]
pub async fn set_caps(
    admin: AdminUser,
    _csrf: AdminCsrf,
    body: Json<CapsBody>,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.daily_spend_cap_micro_usd < 0 || body.anon_daily_token_budget < 0 {
        return Err(err(Status::BadRequest, "bad_caps"));
    }
    let caps = settings::Caps {
        daily_spend_cap_micro_usd: body.daily_spend_cap_micro_usd,
        anon_daily_token_budget: body.anon_daily_token_budget,
    };
    let value = serde_json::to_string(&caps).map_err(|e| internal(e, "serialize"))?;
    settings::put(pool, settings::CAPS_KEY, &value)
        .await
        .map_err(|e| internal(e, "put caps"))?;
    admin_store::audit(
        pool,
        &keyring.audit_mac,
        &admin.user_ref_hash,
        "caps.set",
        "caps",
        &value,
    )
    .await
    .map_err(|e| internal(e, "audit"))?;
    Ok(Json(json!({"ok": true})))
}

// ---------------------------------------------------------------------------
// Model catalog (ZDR policy is data the console maintains)

#[get("/admin/api/models")]
pub async fn list_models(
    _admin: AdminUser,
    pool: &State<PgPool>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let rows: Vec<(String, String, bool, bool)> = sqlx::query_as(
        "SELECT model_id, display_name, zdr, enabled FROM model_catalog ORDER BY display_name",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| internal(e, "catalog"))?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, name, zdr, enabled)| {
                json!({"model_id": id, "display_name": name, "zdr": zdr, "enabled": enabled})
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
pub struct ToggleModelBody {
    pub model_id: String,
    pub enabled: bool,
}

#[post("/admin/api/models/toggle", format = "json", data = "<body>")]
pub async fn toggle_model(
    admin: AdminUser,
    _csrf: AdminCsrf,
    body: Json<ToggleModelBody>,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let res = sqlx::query(
        "UPDATE model_catalog SET enabled = $1, updated_at = now() WHERE model_id = $2",
    )
    .bind(body.enabled)
    .bind(&body.model_id)
    .execute(pool.inner())
    .await
    .map_err(|e| internal(e, "toggle model"))?;
    if res.rows_affected() == 0 {
        return Err(err(Status::NotFound, "not_found"));
    }
    admin_store::audit(
        pool,
        &keyring.audit_mac,
        &admin.user_ref_hash,
        "model.toggle",
        &body.model_id,
        &json!({"enabled": body.enabled}).to_string(),
    )
    .await
    .map_err(|e| internal(e, "audit"))?;
    Ok(Json(json!({"ok": true})))
}

// ---------------------------------------------------------------------------
// Roster

#[get("/admin/api/admins")]
pub async fn list_admins(
    _admin: AdminUser,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let rows = admin_store::list_admins(pool, &keyring.admin_roster_mac)
        .await
        .map_err(|e| internal(e, "list admins"))?;
    Ok(Json(
        rows.into_iter()
            .map(|(r, valid)| {
                json!({
                    "user_ref_hash": r.user_ref_hash,
                    "role": r.role,
                    "granted_by": r.granted_by,
                    "granted_at_unix": r.granted_at_unix,
                    "mac_valid": valid,
                })
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
pub struct GrantBody {
    pub email: String,
}

#[post("/admin/api/admins", format = "json", data = "<body>")]
pub async fn grant_admin(
    admin: AdminUser,
    _csrf: AdminCsrf,
    body: Json<GrantBody>,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(err(Status::BadRequest, "bad_email"));
    }
    // Derive in-enclave; only the hash is stored or returned.
    let user_ref = identity::account_user_ref(&keyring.user_id_namespace, &email);
    let hash = identity::user_ref_hash(&user_ref);
    admin_store::grant(pool, &keyring.admin_roster_mac, &hash, &admin.user_ref_hash)
        .await
        .map_err(|e| internal(e, "grant"))?;
    admin_store::audit(
        pool,
        &keyring.audit_mac,
        &admin.user_ref_hash,
        "admin.grant",
        &hash,
        "{}",
    )
    .await
    .map_err(|e| internal(e, "audit"))?;
    Ok(Json(json!({"ok": true, "user_ref_hash": hash})))
}

#[delete("/admin/api/admins/<user_ref_hash>")]
pub async fn revoke_admin(
    admin: AdminUser,
    _csrf: AdminCsrf,
    user_ref_hash: &str,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Never remove the last valid admin.
    let rows = admin_store::list_admins(pool, &keyring.admin_roster_mac)
        .await
        .map_err(|e| internal(e, "list admins"))?;
    let valid_count = rows.iter().filter(|(_, v)| *v).count();
    let target_valid = rows
        .iter()
        .any(|(r, v)| *v && r.user_ref_hash == user_ref_hash);
    if target_valid && valid_count <= 1 {
        return Err(err(Status::Conflict, "cannot_remove_last_admin"));
    }
    let removed = admin_store::revoke(pool, user_ref_hash)
        .await
        .map_err(|e| internal(e, "revoke"))?;
    if !removed {
        return Err(err(Status::NotFound, "not_found"));
    }
    admin_store::audit(
        pool,
        &keyring.audit_mac,
        &admin.user_ref_hash,
        "admin.revoke",
        user_ref_hash,
        "{}",
    )
    .await
    .map_err(|e| internal(e, "audit"))?;
    Ok(Json(json!({"ok": true})))
}

// ---------------------------------------------------------------------------
// Audit log

#[get("/admin/api/audit?<limit>")]
pub async fn audit_log(
    _admin: AdminUser,
    limit: Option<i64>,
    pool: &State<PgPool>,
    keyring: &State<Keyring>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let limit = limit.unwrap_or(100).clamp(1, 1000);
    let rows = admin_store::audit_list(pool, &keyring.audit_mac, limit)
        .await
        .map_err(|e| internal(e, "audit list"))?;
    Ok(Json(
        rows.into_iter()
            .map(|(r, mac_valid, chain_valid)| {
                json!({
                    "id": r.id,
                    "actor_ref_hash": r.actor_ref_hash,
                    "action": r.action,
                    "subject": r.subject,
                    "detail": r.detail,
                    "created_at_unix": r.created_at_unix,
                    "mac_valid": mac_valid,
                    "chain_valid": chain_valid,
                })
            })
            .collect(),
    ))
}

#[get("/health")]
pub fn health() -> &'static str {
    "ok"
}
