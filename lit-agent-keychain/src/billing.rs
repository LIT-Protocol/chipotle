use crate::{
    actions,
    api::{self, ApiError, ApiResult},
    auth::SameOrigin,
    chipotle::Chipotle,
    config::Config,
    crypto,
    models::Authority,
};
use rocket::{
    http::Status,
    post,
    request::{FromRequest, Outcome},
    serde::json::Json,
    Request, State,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
pub struct Peer(pub String);
#[rocket::async_trait]
impl<'r> FromRequest<'r> for Peer {
    type Error = ();
    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        // Ignore caller-controlled forwarding headers. A reverse proxy shares a
        // bucket unless its trusted network integration supplies a real peer.
        Outcome::Success(Self(crypto::hash_bytes(
            req.remote()
                .map(|a| a.ip().to_string())
                .unwrap_or_else(|| "unknown".into())
                .as_bytes(),
        )))
    }
}
pub async fn reserve(pool: &PgPool, bucket: &str, period: i64, limit: i64) -> Result<(), ApiError> {
    let window = time::OffsetDateTime::now_utc().unix_timestamp() / period;
    let used=sqlx::query_scalar::<_,i64>("INSERT INTO kc_budgets(bucket,period_start,used) VALUES($1,$2,1) ON CONFLICT(bucket,period_start) DO UPDATE SET used=kc_budgets.used+1 WHERE kc_budgets.used<$3 RETURNING used")
        .bind(bucket).bind(window).bind(limit).fetch_optional(pool).await.map_err(api::internal)?;
    if used.is_none() {
        return Err(api::err(Status::TooManyRequests, "budget_exhausted"));
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Execution {
    Authority { manifest: Authority, params: Value },
}
#[post("/api/execute", format = "json", data = "<body>")]
pub async fn execute(
    _origin: SameOrigin,
    peer: Peer,
    body: Json<Execution>,
    cfg: &State<Config>,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
) -> ApiResult<Value> {
    let (code, params, vault) = match body.into_inner() {
        Execution::Authority { manifest, params } => {
            manifest.validate(cfg).map_err(api::invalid)?;
            let vault = manifest.vault_id().map_err(api::invalid)?;
            (
                actions::authority_source(&manifest).map_err(api::invalid)?,
                params,
                vault,
            )
        }
    };
    if params["document"]["kind"] != "login" || params["document"]["vaultId"] != vault {
        return Err(api::err(Status::Forbidden, "login_bootstrap_only"));
    }
    let challenge = params["document"]["challenge"].as_str().unwrap_or("");
    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM kc_challenges WHERE challenge=$1 AND vault_id=$2 AND expires_at>now())")
        .bind(challenge).bind(&vault).fetch_one(pool.inner()).await.map_err(api::internal)?;
    if !pending {
        return Err(api::err(Status::Forbidden, "challenge_used_or_expired"));
    }
    // These counters bound the server-sponsored sign-in bootstrap only. User
    // keys execute directly on Chipotle and are subject to its shared billing.
    reserve(pool, "execution-global", 86400, cfg.daily_execution_limit).await?;
    reserve(
        pool,
        &format!("execution-ip:{}", peer.0),
        3600,
        cfg.hourly_ip_execution_limit,
    )
    .await?;
    reserve(
        pool,
        &format!("execution-vault:{vault}"),
        86400,
        cfg.daily_vault_execution_limit,
    )
    .await?;
    let response = lit
        .execute(&code, &params)
        .await
        .map_err(|_| api::err(Status::BadGateway, "lit_unavailable"))?;
    Ok(Json(
        json!({"actionCid":actions::cid(&code),"response":response}),
    ))
}
