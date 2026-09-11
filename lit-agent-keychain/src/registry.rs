use crate::{
    actions,
    api::{self, ApiError, ApiResult},
    auth::{SameOrigin, Session},
    billing,
    chipotle::Chipotle,
    config::Config,
    crypto,
    models::{field, number, valid_hex, Authority, Manifest, Signed},
};
use anyhow::{bail, Result};
use rocket::{get, http::Status, post, put, serde::json::Json, State};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};

async fn key_for_vault(pool: &PgPool, lit: &Chipotle, vault: &str) -> Result<String, ApiError> {
    let cid: String = sqlx::query_scalar("SELECT authority_cid FROM kc_vaults WHERE id=$1")
        .bind(vault)
        .fetch_one(pool)
        .await
        .map_err(api::internal)?;
    lit.public_key(&cid)
        .await
        .map_err(|_| api::err(Status::BadGateway, "lit_unavailable"))
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    vault: &str,
    event: &str,
    hash: &str,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO kc_audit(vault_id,event,object_hash) VALUES($1,$2,$3)")
        .bind(vault)
        .bind(event)
        .bind(hash)
        .execute(&mut **tx)
        .await
        .map_err(api::internal)?;
    Ok(())
}
pub async fn selected(pool: &PgPool, scope: &str) -> Result<Option<Value>, ApiError> {
    sqlx::query_scalar("SELECT p.signed FROM kc_registry r JOIN kc_policies p ON p.hash=r.policy_hash WHERE r.scope=$1")
        .bind(scope).fetch_optional(pool).await.map_err(api::internal)
}
#[get("/api/registry/credentials/<vault>")]
pub async fn credentials(vault: &str, pool: &State<PgPool>) -> ApiResult<Value> {
    if !valid_hex(vault, 32) {
        return Err(api::err(Status::BadRequest, "invalid_id"));
    }
    Ok(Json(
        json!({"policy":selected(pool,&format!("credentials:{vault}")).await?}),
    ))
}
#[get("/api/registry/secrets/<secret>")]
pub async fn policy(secret: &str, pool: &State<PgPool>) -> ApiResult<Value> {
    if !valid_hex(secret, 32) {
        return Err(api::err(Status::BadRequest, "invalid_id"));
    }
    let value = selected(pool, &format!("secret:{secret}"))
        .await?
        .ok_or_else(|| api::err(Status::NotFound, "not_found"))?;
    Ok(Json(json!({"policy":value})))
}
fn validate_policy(
    signed: &Signed,
    vault: &str,
    key: &str,
    secret: Option<(&str, &str)>,
    allow_expired: bool,
) -> Result<()> {
    crypto::verify_signed(signed, key, vault)?;
    let p = &signed.document;
    let max = if let Some((id, cid)) = secret {
        if field(p, "kind")? != "policy"
            || field(p, "secretId")? != id
            || field(p, "actionCid")? != cid
        {
            bail!("policy mismatch");
        }
        90 * 86400
    } else {
        if field(p, "kind")? != "credentials" {
            bail!("credentials required");
        }
        366 * 86400
    };
    let start = number(p, "notBefore")?;
    if secret.is_none() && p.get("expiresAt") == Some(&Value::Null) {
        if number(p, "epoch")? < 1 || start > time::OffsetDateTime::now_utc().unix_timestamp() {
            bail!("invalid credentials window");
        }
        return Ok(());
    }
    let end = number(p, "expiresAt")?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    if number(p, "epoch")? < 1
        || start > now + 30
        || end <= start
        || end - start > max
        || (!allow_expired && end <= now)
    {
        bail!("invalid policy window");
    }
    Ok(())
}
async fn commit_policy(
    tx: &mut Transaction<'_, Postgres>,
    vault: &str,
    scope: &str,
    signed: &Signed,
    restoring: bool,
) -> Result<(), ApiError> {
    let epoch = number(&signed.document, "epoch").map_err(api::invalid)?;
    let previous = signed.document.get("previousHash").and_then(Value::as_str);
    let current: Option<(i64, String)> =
        sqlx::query_as("SELECT epoch,policy_hash FROM kc_registry WHERE scope=$1 FOR UPDATE")
            .bind(scope)
            .fetch_optional(&mut **tx)
            .await
            .map_err(api::internal)?;
    match current {
        Some((old, hash)) if epoch == old + 1 && previous == Some(hash.as_str()) => (),
        None if (epoch == 1 && previous.is_none()) || restoring => (),
        _ => {
            return Err(api::err(
                Status::Conflict,
                "policy_changed_refresh_required",
            ))
        }
    }
    let hash = crypto::digest(&signed.document).map_err(api::invalid)?;
    sqlx::query("INSERT INTO kc_policies(hash,vault_id,scope,epoch,signed) VALUES($1,$2,$3,$4,$5) ON CONFLICT(hash) DO NOTHING")
        .bind(&hash).bind(vault).bind(scope).bind(epoch).bind(serde_json::to_value(signed).map_err(api::invalid)?).execute(&mut **tx).await.map_err(api::internal)?;
    sqlx::query("INSERT INTO kc_registry(scope,vault_id,policy_hash,epoch) VALUES($1,$2,$3,$4) ON CONFLICT(scope) DO UPDATE SET policy_hash=$3,epoch=$4")
        .bind(scope).bind(vault).bind(&hash).bind(epoch).execute(&mut **tx).await.map_err(api::internal)?;
    audit(tx, vault, "policy_updated", &hash).await
}
async fn lock_vault(tx: &mut Transaction<'_, Postgres>, vault: &str) -> Result<(), ApiError> {
    sqlx::query("SELECT id FROM kc_vaults WHERE id=$1 FOR UPDATE")
        .bind(vault)
        .fetch_one(&mut **tx)
        .await
        .map_err(api::internal)?;
    Ok(())
}
#[put("/api/credentials", format = "json", data = "<body>")]
pub async fn update_credentials(
    _origin: SameOrigin,
    session: Session,
    body: Json<Signed>,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
) -> ApiResult<Value> {
    let key = key_for_vault(pool, lit, &session.vault_id).await?;
    validate_policy(&body, &session.vault_id, &key, None, false).map_err(api::denied)?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    lock_vault(&mut tx, &session.vault_id).await?;
    commit_policy(
        &mut tx,
        &session.vault_id,
        &format!("credentials:{}", session.vault_id),
        &body,
        false,
    )
    .await?;
    // Session cookies are not owner authority, but end existing metadata sessions after recovery changes.
    sqlx::query("DELETE FROM kc_sessions WHERE vault_id=$1")
        .bind(&session.vault_id)
        .execute(&mut *tx)
        .await
        .map_err(api::internal)?;
    tx.commit().await.map_err(api::internal)?;
    Ok(Json(json!({"ok":true,"signInRequired":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretWrite {
    pub manifest: Signed,
    pub envelope: Signed,
    pub policy: Signed,
}
struct Validated {
    manifest: Manifest,
    cid: String,
    name: String,
    version: i64,
    envelope_hash: String,
}
fn validate_write(
    body: &SecretWrite,
    vault: &str,
    key: &str,
    cfg: &Config,
    restoring: bool,
) -> Result<Validated> {
    crypto::verify_signed(&body.manifest, key, vault)?;
    crypto::verify_signed(&body.envelope, key, vault)?;
    if field(&body.manifest.document, "kind")? != "manifest"
        || field(&body.envelope.document, "kind")? != "envelope"
    {
        bail!("invalid document kinds");
    }
    let manifest: Manifest = serde_json::from_value(
        body.manifest
            .document
            .get("manifest")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing manifest"))?,
    )?;
    manifest.validate(cfg)?;
    if manifest.vault_id != vault {
        bail!("wrong vault");
    }
    let cid = actions::cid(&actions::secret_source(&manifest)?);
    if field(&body.manifest.document, "actionCid")? != cid {
        bail!("wrong CID");
    }
    let meta = body
        .envelope
        .document
        .get("metadata")
        .ok_or_else(|| anyhow::anyhow!("missing metadata"))?;
    if field(meta, "secretId")? != manifest.secret_id
        || field(meta, "vaultId")? != vault
        || field(meta, "actionCid")? != cid
        || field(meta, "release")? != manifest.release
    {
        bail!("envelope mismatch");
    }
    let version = number(meta, "version")?;
    let name = field(meta, "name")?.to_owned();
    if version < 1
        || name.is_empty()
        || name.len() > 64
        || !name.as_bytes()[0].is_ascii_uppercase()
        || !name
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
    {
        bail!("invalid envelope metadata");
    }
    validate_policy(
        &body.policy,
        vault,
        key,
        Some((&manifest.secret_id, &cid)),
        restoring,
    )?;
    Ok(Validated {
        manifest,
        cid,
        name,
        version,
        envelope_hash: crypto::digest(&body.envelope.document)?,
    })
}
async fn write_secret(
    session: &Session,
    body: SecretWrite,
    pool: &PgPool,
    lit: &Chipotle,
    cfg: &Config,
    restoring: bool,
) -> ApiResult<Value> {
    let key = key_for_vault(pool, lit, &session.vault_id).await?;
    let checked =
        validate_write(&body, &session.vault_id, &key, cfg, restoring).map_err(api::denied)?;
    if !restoring && checked.version != 1 {
        return Err(api::err(Status::BadRequest, "initial_version_must_be_one"));
    }
    let mut tx = pool.begin().await.map_err(api::internal)?;
    lock_vault(&mut tx, &session.vault_id).await?;
    let authority_cid: String =
        sqlx::query_scalar("SELECT authority_cid FROM kc_vaults WHERE id=$1")
            .bind(&session.vault_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(api::internal)?;
    if checked.manifest.authority_cid != authority_cid {
        return Err(api::err(Status::Forbidden, "wrong_authority"));
    }
    // Retrying a partially completed backup must not roll the registry back.
    // An already-present exact envelope is a no-op, even after later rotation.
    if restoring {
        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT s.action_cid,e.envelope_hash FROM kc_secrets s JOIN kc_envelopes e ON e.secret_id=s.id WHERE s.id=$1 AND s.vault_id=$2 AND e.version=$3",
        ).bind(&checked.manifest.secret_id).bind(&session.vault_id).bind(checked.version)
            .fetch_optional(&mut *tx).await.map_err(api::internal)?;
        if existing.as_ref() == Some(&(checked.cid.clone(), checked.envelope_hash.clone())) {
            return Ok(Json(
                json!({"secretId":checked.manifest.secret_id,"actionCid":checked.cid,"alreadyPresent":true}),
            ));
        }
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM kc_secrets WHERE vault_id=$1")
        .bind(&session.vault_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(api::internal)?;
    if count >= cfg.max_secrets {
        return Err(api::err(Status::Conflict, "secret_limit"));
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM kc_secrets WHERE id=$1 OR (vault_id=$2 AND name=$3))",
    )
    .bind(&checked.manifest.secret_id)
    .bind(&session.vault_id)
    .bind(&checked.name)
    .fetch_one(&mut *tx)
    .await
    .map_err(api::internal)?;
    if exists {
        return Err(api::err(Status::Conflict, "secret_exists"));
    }
    sqlx::query("INSERT INTO kc_secrets(id,vault_id,name,manifest,action_cid,current_version) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(&checked.manifest.secret_id).bind(&session.vault_id).bind(&checked.name).bind(serde_json::to_value(&body.manifest).map_err(api::invalid)?)
        .bind(&checked.cid).bind(checked.version).execute(&mut *tx).await.map_err(api::internal)?;
    sqlx::query(
        "INSERT INTO kc_envelopes(secret_id,version,envelope_hash,signed) VALUES($1,$2,$3,$4)",
    )
    .bind(&checked.manifest.secret_id)
    .bind(checked.version)
    .bind(&checked.envelope_hash)
    .bind(serde_json::to_value(&body.envelope).map_err(api::invalid)?)
    .execute(&mut *tx)
    .await
    .map_err(api::internal)?;
    commit_policy(
        &mut tx,
        &session.vault_id,
        &format!("secret:{}", checked.manifest.secret_id),
        &body.policy,
        restoring,
    )
    .await?;
    audit(
        &mut tx,
        &session.vault_id,
        if restoring {
            "secret_restored"
        } else {
            "secret_created"
        },
        &checked.envelope_hash,
    )
    .await?;
    tx.commit().await.map_err(api::internal)?;
    Ok(Json(
        json!({"secretId":checked.manifest.secret_id,"actionCid":checked.cid}),
    ))
}
#[post("/api/secrets", format = "json", data = "<body>")]
pub async fn create(
    _origin: SameOrigin,
    session: Session,
    body: Json<SecretWrite>,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    write_secret(&session, body.into_inner(), pool, lit, cfg, false).await
}
#[post("/api/restore", format = "json", data = "<body>")]
pub async fn restore(
    _origin: SameOrigin,
    session: Session,
    body: Json<SecretWrite>,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    write_secret(&session, body.into_inner(), pool, lit, cfg, true).await
}
#[get("/api/secrets")]
pub async fn list(session: Session, pool: &State<PgPool>) -> ApiResult<Value> {
    let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('secretId',s.id,'name',s.name,'actionCid',s.action_cid,'version',s.current_version,'release',s.manifest->'document'->'manifest'->'release','disabled',p.signed->'document'->'disabled','expiresAt',p.signed->'document'->'expiresAt','agentCount',jsonb_array_length(p.signed->'document'->'grants')) FROM kc_secrets s JOIN kc_registry r ON r.scope='secret:'||s.id JOIN kc_policies p ON p.hash=r.policy_hash WHERE s.vault_id=$1 ORDER BY s.created_at,s.id")
        .bind(&session.vault_id).fetch_all(pool.inner()).await.map_err(api::internal)?;
    Ok(Json(json!({"secrets":rows})))
}
#[get("/api/secrets/<secret>/bundle")]
pub async fn bundle(secret: &str, pool: &State<PgPool>) -> ApiResult<Value> {
    if !valid_hex(secret, 32) {
        return Err(api::err(Status::BadRequest, "invalid_id"));
    }
    let row:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('manifest',s.manifest,'envelope',e.signed,'policy',p.signed) FROM kc_secrets s JOIN kc_envelopes e ON e.secret_id=s.id AND e.version=s.current_version JOIN kc_registry r ON r.scope='secret:'||s.id JOIN kc_policies p ON p.hash=r.policy_hash WHERE s.id=$1")
        .bind(secret).fetch_optional(pool.inner()).await.map_err(api::internal)?;
    Ok(Json(
        row.ok_or_else(|| api::err(Status::NotFound, "not_found"))?,
    ))
}
#[put("/api/secrets/<secret>/policy", format = "json", data = "<body>")]
pub async fn update_policy(
    _origin: SameOrigin,
    session: Session,
    secret: &str,
    body: Json<Signed>,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
) -> ApiResult<Value> {
    let cid: Option<String> =
        sqlx::query_scalar("SELECT action_cid FROM kc_secrets WHERE id=$1 AND vault_id=$2")
            .bind(secret)
            .bind(&session.vault_id)
            .fetch_optional(pool.inner())
            .await
            .map_err(api::internal)?;
    let cid = cid.ok_or_else(|| api::err(Status::NotFound, "not_found"))?;
    let key = key_for_vault(pool, lit, &session.vault_id).await?;
    validate_policy(&body, &session.vault_id, &key, Some((secret, &cid)), false)
        .map_err(api::denied)?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    lock_vault(&mut tx, &session.vault_id).await?;
    commit_policy(
        &mut tx,
        &session.vault_id,
        &format!("secret:{secret}"),
        &body,
        false,
    )
    .await?;
    tx.commit().await.map_err(api::internal)?;
    Ok(Json(json!({"ok":true})))
}
#[post("/api/secrets/<secret>/rotate", format = "json", data = "<body>")]
pub async fn rotate(
    _origin: SameOrigin,
    session: Session,
    secret: &str,
    body: Json<SecretWrite>,
    pool: &State<PgPool>,
    lit: &State<Chipotle>,
    cfg: &State<Config>,
) -> ApiResult<Value> {
    let key = key_for_vault(pool, lit, &session.vault_id).await?;
    let checked =
        validate_write(&body, &session.vault_id, &key, cfg, false).map_err(api::denied)?;
    if checked.manifest.secret_id != secret {
        return Err(api::err(Status::Forbidden, "wrong_secret"));
    }
    let mut tx = pool.begin().await.map_err(api::internal)?;
    lock_vault(&mut tx, &session.vault_id).await?;
    let old: Option<(i64, String)> = sqlx::query_as(
        "SELECT current_version,action_cid FROM kc_secrets WHERE id=$1 AND vault_id=$2 FOR UPDATE",
    )
    .bind(secret)
    .bind(&session.vault_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(api::internal)?;
    let (version, cid) = old.ok_or_else(|| api::err(Status::NotFound, "not_found"))?;
    if version >= 100 || checked.version != version + 1 || checked.cid != cid {
        return Err(api::err(Status::Conflict, "version_changed_or_limit"));
    }
    sqlx::query(
        "INSERT INTO kc_envelopes(secret_id,version,envelope_hash,signed) VALUES($1,$2,$3,$4)",
    )
    .bind(secret)
    .bind(checked.version)
    .bind(&checked.envelope_hash)
    .bind(serde_json::to_value(&body.envelope).map_err(api::invalid)?)
    .execute(&mut *tx)
    .await
    .map_err(api::internal)?;
    sqlx::query("UPDATE kc_secrets SET current_version=$2,name=$3 WHERE id=$1")
        .bind(secret)
        .bind(checked.version)
        .bind(&checked.name)
        .execute(&mut *tx)
        .await
        .map_err(api::internal)?;
    commit_policy(
        &mut tx,
        &session.vault_id,
        &format!("secret:{secret}"),
        &body.policy,
        false,
    )
    .await?;
    audit(
        &mut tx,
        &session.vault_id,
        "secret_rotated",
        &checked.envelope_hash,
    )
    .await?;
    tx.commit().await.map_err(api::internal)?;
    Ok(Json(json!({"ok":true})))
}
#[get("/api/audit?<before>")]
pub async fn audit_log(
    session: Session,
    before: Option<i64>,
    pool: &State<PgPool>,
) -> ApiResult<Value> {
    let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'event',event,'objectHash',object_hash,'createdAt',created_at) FROM kc_audit WHERE vault_id=$1 AND id<$2 ORDER BY id DESC LIMIT 100")
        .bind(&session.vault_id).bind(before.unwrap_or(i64::MAX)).fetch_all(pool.inner()).await.map_err(api::internal)?;
    Ok(Json(json!({"events":rows})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialBackup {
    authority: Authority,
    credentials: Signed,
}
/// Restore a previously approved credential policy after complete DB loss.
/// Existing vaults are never changed, including their initial/null policy state.
#[post("/auth/restore-credentials", format = "json", data = "<body>")]
pub async fn restore_credentials(
    _origin: SameOrigin,
    peer: billing::Peer,
    body: Json<CredentialBackup>,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    lit: &State<Chipotle>,
) -> ApiResult<Value> {
    body.authority.validate(cfg).map_err(api::invalid)?;
    billing::reserve(pool, "challenge-global", 3600, 10000).await?;
    billing::reserve(pool, &format!("challenge:{}", peer.0), 3600, 100).await?;
    let vault = body.authority.vault_id().map_err(api::invalid)?;
    let cid = actions::cid(&actions::authority_source(&body.authority).map_err(api::invalid)?);
    let key = lit
        .public_key(&cid)
        .await
        .map_err(|_| api::err(Status::BadGateway, "lit_unavailable"))?;
    validate_policy(&body.credentials, &vault, &key, None, false).map_err(api::denied)?;
    let mut tx = pool.begin().await.map_err(api::internal)?;
    let inserted = sqlx::query("INSERT INTO kc_vaults(id,authority,authority_cid) VALUES($1,$2,$3) ON CONFLICT(id) DO NOTHING")
        .bind(&vault).bind(serde_json::to_value(&body.authority).map_err(api::invalid)?).bind(&cid)
        .execute(&mut *tx).await.map_err(api::internal)?.rows_affected();
    if inserted == 1 {
        commit_policy(
            &mut tx,
            &vault,
            &format!("credentials:{vault}"),
            &body.credentials,
            true,
        )
        .await?;
    }
    tx.commit().await.map_err(api::internal)?;
    Ok(Json(json!({"restored":inserted == 1})))
}
