use crate::{
    actions,
    api::{self, ApiError, ApiResult},
    auth::SameOrigin,
    chipotle::Chipotle,
    config::Config,
    crypto,
    models::{Authority, Manifest},
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
    Secret { manifest: Manifest, params: Value },
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
        Execution::Secret { manifest, params } => {
            manifest.validate(cfg).map_err(api::invalid)?;
            (
                actions::secret_source(&manifest).map_err(api::invalid)?,
                params,
                manifest.vault_id,
            )
        }
    };
    // Every attempt consumes capacity, including invalid proofs and failures.
    // The billing key stays server-side; clients cannot bypass these counters.
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
