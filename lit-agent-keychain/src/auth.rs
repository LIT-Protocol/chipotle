use crate::{
    actions,
    api::{self, ApiError, ApiResult},
    billing,
    chipotle::Chipotle,
    config::Config,
    crypto,
    models::{field, number, Authority, Signed},
};
use rocket::{
    get,
    http::{Cookie, CookieJar, SameSite, Status},
    post,
    request::{FromRequest, Outcome},
    serde::json::Json,
    Request, State,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

const COOKIE: &str = "kc_session";
#[derive(Clone)]
pub struct Session {
    pub vault_id: String,
}
#[rocket::async_trait]
impl<'r> FromRequest<'r> for Session {
    type Error = ();
    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        let Some(cookie) = req.cookies().get(COOKIE) else {
            return Outcome::Error((Status::Unauthorized, ()));
        };
        let Some(pool) = req.rocket().state::<PgPool>() else {
            return Outcome::Error((Status::InternalServerError, ()));
        };
        if cookie.value().len() != 64 {
            return Outcome::Error((Status::Unauthorized, ()));
        }
        match sqlx::query_scalar::<_, String>(
            "SELECT vault_id FROM kc_sessions WHERE token_hash=$1 AND expires_at>now()",
        )
        .bind(crypto::hash_bytes(cookie.value().as_bytes()))
        .fetch_optional(pool)
        .await
        {
            Ok(Some(vault_id)) => Outcome::Success(Session { vault_id }),
            Ok(None) => Outcome::Error((Status::Unauthorized, ())),
            Err(_) => Outcome::Error((Status::ServiceUnavailable, ())),
        }
    }
}
// Mutations require JSON and same-origin browser requests. No ambient-session
// authorization can produce owner receipts; this guard also protects metadata.
pub struct SameOrigin;
#[rocket::async_trait]
impl<'r> FromRequest<'r> for SameOrigin {
    type Error = ();
    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        let Some(cfg) = req.rocket().state::<Config>() else {
            return Outcome::Error((Status::InternalServerError, ()));
        };
        if req
            .headers()
            .get_one("Origin")
            .is_some_and(|o| o != cfg.public_base_url)
            || req.headers().get_one("Sec-Fetch-Site") == Some("cross-site")
        {
            return Outcome::Error((Status::Forbidden, ()));
        }
        Outcome::Success(Self)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Login {
    authority: Authority,
    authorization: Signed,
}

#[post("/auth/challenge", format = "json", data = "<authority>")]
pub async fn challenge(
    _origin: SameOrigin,
    peer: billing::Peer,
    authority: Json<Authority>,
    cfg: &State<Config>,
    pool: &State<PgPool>,
) -> ApiResult<Value> {
    authority.validate(cfg).map_err(api::invalid)?;
    billing::reserve(pool, "challenge-global", 3600, 10000).await?;
    billing::reserve(pool, &format!("challenge:{}", peer.0), 3600, 100).await?;
    let vault_id = authority.vault_id().map_err(api::invalid)?;
    let nonce = crypto::random_token();
    let expires = time::OffsetDateTime::now_utc() + time::Duration::minutes(5);
    sqlx::query("INSERT INTO kc_challenges(challenge,vault_id,expires_at) VALUES($1,$2,$3)")
        .bind(&nonce)
        .bind(&vault_id)
        .bind(expires)
        .execute(pool.inner())
        .await
        .map_err(api::internal)?;
    Ok(Json(
        json!({"v":2,"domain":"lit-keychain/v2","kind":"login","vaultId":vault_id,"challenge":nonce,"expiresAt":expires.unix_timestamp()}),
    ))
}
#[post("/auth/login", format = "json", data = "<body>")]
pub async fn login(
    _origin: SameOrigin,
    body: Json<Login>,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    lit: &State<Chipotle>,
    cookies: &CookieJar<'_>,
) -> ApiResult<Value> {
    body.authority.validate(cfg).map_err(api::invalid)?;
    let vault_id = body.authority.vault_id().map_err(api::invalid)?;
    let code = actions::authority_source(&body.authority).map_err(api::invalid)?;
    let cid = actions::cid(&code);
    let key = lit
        .public_key(&cid)
        .await
        .map_err(|_| api::err(Status::BadGateway, "lit_unavailable"))?;
    crypto::verify_signed(&body.authorization, &key, &vault_id).map_err(api::denied)?;
    let doc = &body.authorization.document;
    if field(doc, "kind").map_err(api::invalid)? != "login"
        || number(doc, "expiresAt").map_err(api::invalid)?
            <= time::OffsetDateTime::now_utc().unix_timestamp()
    {
        return Err(api::err(Status::Forbidden, "login_expired"));
    }
    let mut tx = pool.begin().await.map_err(api::internal)?;
    let deleted = sqlx::query(
        "DELETE FROM kc_challenges WHERE challenge=$1 AND vault_id=$2 AND expires_at>now()",
    )
    .bind(field(doc, "challenge").map_err(api::invalid)?)
    .bind(&vault_id)
    .execute(&mut *tx)
    .await
    .map_err(api::internal)?;
    if deleted.rows_affected() != 1 {
        return Err(api::err(Status::Forbidden, "challenge_used_or_expired"));
    }
    sqlx::query("INSERT INTO kc_vaults(id,authority,authority_cid) VALUES($1,$2,$3) ON CONFLICT(id) DO NOTHING")
        .bind(&vault_id).bind(serde_json::to_value(&body.authority).map_err(api::invalid)?).bind(cid).execute(&mut *tx).await.map_err(api::internal)?;
    let token = crypto::random_token();
    sqlx::query("INSERT INTO kc_sessions(token_hash,vault_id,expires_at) VALUES($1,$2,now()+interval '12 hours')")
        .bind(crypto::hash_bytes(token.as_bytes())).bind(&vault_id).execute(&mut *tx).await.map_err(api::internal)?;
    sqlx::query("INSERT INTO kc_audit(vault_id,event) VALUES($1,'login')")
        .bind(&vault_id)
        .execute(&mut *tx)
        .await
        .map_err(api::internal)?;
    tx.commit().await.map_err(api::internal)?;
    cookies.add(
        Cookie::build((COOKIE, token))
            .http_only(true)
            .secure(cfg.secure_cookies)
            .same_site(SameSite::Strict)
            .path("/")
            .build(),
    );
    Ok(Json(json!({"vaultId":vault_id,"authority":body.authority})))
}
#[get("/api/me")]
pub async fn me(session: Session, pool: &State<PgPool>) -> ApiResult<Value> {
    let authority: Value = sqlx::query_scalar("SELECT authority FROM kc_vaults WHERE id=$1")
        .bind(&session.vault_id)
        .fetch_one(pool.inner())
        .await
        .map_err(api::internal)?;
    Ok(Json(
        json!({"vaultId":session.vault_id,"authority":authority}),
    ))
}
#[post("/auth/logout")]
pub async fn logout(
    _origin: SameOrigin,
    pool: &State<PgPool>,
    cookies: &CookieJar<'_>,
) -> Result<Status, ApiError> {
    if let Some(cookie) = cookies.get(COOKIE) {
        sqlx::query("DELETE FROM kc_sessions WHERE token_hash=$1")
            .bind(crypto::hash_bytes(cookie.value().as_bytes()))
            .execute(pool.inner())
            .await
            .map_err(api::internal)?;
    }
    cookies.remove(Cookie::build(COOKIE).path("/"));
    Ok(Status::NoContent)
}

/// Discover a synced passkey's public owner descriptor on a fresh device.
/// This is only discovery; login still requires action-verified possession.
#[get("/api/passkeys/<credential_id>")]
pub async fn passkey_lookup(credential_id: &str, pool: &State<PgPool>) -> ApiResult<Value> {
    if credential_id.is_empty()
        || credential_id.len() > 1400
        || !credential_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(api::err(Status::BadRequest, "invalid_credential"));
    }
    let row:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('authority',v.authority,'owner',v.authority->'owner') FROM kc_vaults v WHERE v.authority->'owner'->>'kind'='passkey' AND v.authority->'owner'->>'credentialId'=$1 LIMIT 1")
        .bind(credential_id).fetch_optional(pool.inner()).await.map_err(api::internal)?;
    if let Some(value) = row {
        return Ok(Json(value));
    }
    let row:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('authority',v.authority,'owner',o.owner) FROM kc_registry r JOIN kc_policies p ON p.hash=r.policy_hash JOIN kc_vaults v ON v.id=r.vault_id CROSS JOIN LATERAL jsonb_array_elements(p.signed->'document'->'owners') o(owner) WHERE r.scope='credentials:'||v.id AND o.owner->>'kind'='passkey' AND o.owner->>'credentialId'=$1 LIMIT 1")
        .bind(credential_id).fetch_optional(pool.inner()).await.map_err(api::internal)?;
    Ok(Json(row.ok_or_else(|| {
        api::err(Status::NotFound, "passkey_not_registered")
    })?))
}
