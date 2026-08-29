//! Tenants: one per user. Provisioning mints a vault PKP + group on the app's
//! Chipotle account, attaches the pinned encrypt/reader actions, and mints a
//! service usage key the control plane uses to run the encrypt action.
//!
//! Also hosts the tenant-owned action registry (in-TEE-only tier): customer
//! CIDs attached to the tenant group so they can `Lit.Actions.Decrypt` vault
//! ciphertexts directly.

use anyhow::{Context, Result};
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{delete, get, post, State};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::actions::{hashed_cid, ActionSet};
use crate::api::{err, err_detail, internal, upstream, ApiError, ApiResult};
use crate::auth::User;
use crate::chipotle::ChipotleClient;
use crate::config::Config;
use crate::crypto;

#[derive(Debug, Clone)]
pub struct Tenant {
    pub id: Uuid,
    pub user_id: Uuid,
    pub pkp_id: String,
    pub group_id: i64,
    pub service_key_ciphertext: Vec<u8>,
    pub service_key_nonce: Vec<u8>,
    pub reader_cid: String,
    pub encrypt_cid: String,
}

impl Tenant {
    pub fn group_id_u64(&self) -> u64 {
        self.group_id.max(0) as u64
    }
}

/// Serializes tenant provisioning so two concurrent first requests from one
/// user don't both mint a PKP.
#[derive(Default)]
pub struct ProvisionLock(Mutex<()>);

const TENANT_COLS: &str = "id, user_id, pkp_id, group_id, service_key_ciphertext, service_key_nonce, reader_cid, encrypt_cid";

fn row_to_tenant(r: sqlx::postgres::PgRow) -> Tenant {
    Tenant {
        id: r.get("id"),
        user_id: r.get("user_id"),
        pkp_id: r.get("pkp_id"),
        group_id: r.get("group_id"),
        service_key_ciphertext: r.get("service_key_ciphertext"),
        service_key_nonce: r.get("service_key_nonce"),
        reader_cid: r.get("reader_cid"),
        encrypt_cid: r.get("encrypt_cid"),
    }
}

pub async fn find_by_user(pool: &PgPool, user_id: Uuid) -> Result<Option<Tenant>> {
    let row = sqlx::query(&format!(
        "SELECT {TENANT_COLS} FROM tenants WHERE user_id = $1"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(row_to_tenant))
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Tenant>> {
    let row = sqlx::query(&format!("SELECT {TENANT_COLS} FROM tenants WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_tenant))
}

/// Decrypt the tenant's service usage key (used to run the encrypt action).
pub fn service_key(cfg: &Config, tenant: &Tenant) -> Result<String> {
    crypto::decrypt_usage_key(
        &cfg.usage_key_encryption_key,
        &tenant.service_key_nonce,
        &tenant.service_key_ciphertext,
    )
    .context("decrypting tenant service key")
}

/// Register a CID on the app's Chipotle account exactly once. Idempotent:
/// tracked in `account_actions`, and an "already exists" upstream reply is
/// treated as success.
pub async fn ensure_action_registered(
    pool: &PgPool,
    cfg: &Config,
    chipotle: &ChipotleClient,
    cid: &str,
    name: &str,
    description: &str,
) -> Result<(), ApiError> {
    let (known,): (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM account_actions WHERE cid = $1)")
            .bind(cid)
            .fetch_one(pool)
            .await
            .map_err(|e| internal("account_actions_lookup_failed", e))?;
    if known {
        return Ok(());
    }
    match chipotle
        .add_action(&cfg.chipotle_master_api_key, cid, name, description)
        .await
    {
        Ok(()) => {}
        Err(e) if e.is_already_exists() => {
            tracing::info!(cid, "action already registered on account; recording");
        }
        Err(e) => return Err(upstream("chipotle_add_action_failed", &e)),
    }
    sqlx::query("INSERT INTO account_actions (cid) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(cid)
        .execute(pool)
        .await
        .map_err(|e| internal("account_actions_insert_failed", e))?;
    Ok(())
}

/// Return the user's tenant, provisioning it on first use.
pub async fn ensure_tenant(
    pool: &PgPool,
    cfg: &Config,
    chipotle: &ChipotleClient,
    actions: &ActionSet,
    lock: &ProvisionLock,
    user_id: Uuid,
) -> Result<Tenant, ApiError> {
    if let Some(t) = find_by_user(pool, user_id)
        .await
        .map_err(|e| internal("tenant_lookup_failed", e))?
    {
        return Ok(t);
    }
    let _guard = lock.0.lock().await;
    if let Some(t) = find_by_user(pool, user_id)
        .await
        .map_err(|e| internal("tenant_lookup_failed", e))?
    {
        return Ok(t);
    }
    provision(pool, cfg, chipotle, actions, user_id).await
}

async fn provision(
    pool: &PgPool,
    cfg: &Config,
    chipotle: &ChipotleClient,
    actions: &ActionSet,
    user_id: Uuid,
) -> Result<Tenant, ApiError> {
    let master = cfg.chipotle_master_api_key.as_str();
    let short = &user_id.to_string()[..8];
    tracing::info!(%user_id, "provisioning tenant");

    let pkp_id = chipotle
        .create_wallet(master)
        .await
        .map_err(|e| upstream("chipotle_create_wallet_failed", &e))?;

    let group_id = chipotle
        .add_group(
            master,
            &format!("lit-secrets:{short}"),
            &format!("lit-secrets vault group for user {user_id}"),
            std::slice::from_ref(&pkp_id),
        )
        .await
        .map_err(|e| upstream("chipotle_add_group_failed", &e))?;

    ensure_action_registered(
        pool,
        cfg,
        chipotle,
        &actions.encrypt_cid,
        "lit-secrets encrypt",
        "Seals a secret value to a tenant vault PKP",
    )
    .await?;
    ensure_action_registered(
        pool,
        cfg,
        chipotle,
        &actions.reader_cid,
        "lit-secrets reader",
        &format!(
            "Grant-gated plaintext reader (grant signer {})",
            actions.grant_signer
        ),
    )
    .await?;
    for cid in [&actions.encrypt_cid, &actions.reader_cid] {
        chipotle
            .add_action_to_group(master, group_id, cid)
            .await
            .map_err(|e| upstream("chipotle_add_action_to_group_failed", &e))?;
    }

    let service_key = chipotle
        .add_usage_api_key(
            master,
            &format!("lit-secrets service {short}"),
            "Control-plane key: runs the encrypt action for this tenant",
            &[group_id],
        )
        .await
        .map_err(|e| upstream("chipotle_add_usage_api_key_failed", &e))?;
    let (nonce, ciphertext) =
        crypto::encrypt_usage_key(&cfg.usage_key_encryption_key, &service_key)
            .map_err(|e| internal("service_key_encrypt_failed", e))?;

    let id = Uuid::new_v4();
    let row = sqlx::query(&format!(
        "INSERT INTO tenants (id, user_id, pkp_id, group_id, service_key_ciphertext, service_key_nonce, reader_cid, encrypt_cid)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING {TENANT_COLS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(&pkp_id)
    .bind(group_id as i64)
    .bind(ciphertext)
    .bind(nonce)
    .bind(&actions.reader_cid)
    .bind(&actions.encrypt_cid)
    .fetch_one(pool)
    .await
    .map_err(|e| internal("tenant_insert_failed", e))?;

    tracing::info!(%user_id, tenant_id = %id, group_id, "tenant provisioned");
    Ok(row_to_tenant(row))
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct TenantResponse {
    pub provisioned: bool,
    pub tenant_id: Option<Uuid>,
    pub pkp_id: Option<String>,
    pub group_id: Option<i64>,
    pub reader_cid: String,
    pub encrypt_cid: String,
    pub grant_signer: String,
    pub chipotle_api_base_url: String,
    /// True when this deployment's reader CID differs from the one attached to
    /// the tenant group (grant signer rotated) — grants will fail until the
    /// new reader is attached.
    pub reader_cid_stale: bool,
}

fn tenant_response(
    t: Option<&Tenant>,
    actions: &ActionSet,
    chipotle: &ChipotleClient,
) -> TenantResponse {
    TenantResponse {
        provisioned: t.is_some(),
        tenant_id: t.map(|t| t.id),
        pkp_id: t.map(|t| t.pkp_id.clone()),
        group_id: t.map(|t| t.group_id),
        reader_cid: actions.reader_cid.clone(),
        encrypt_cid: actions.encrypt_cid.clone(),
        grant_signer: actions.grant_signer.clone(),
        chipotle_api_base_url: chipotle.base_url().to_string(),
        reader_cid_stale: t
            .map(|t| t.reader_cid != actions.reader_cid)
            .unwrap_or(false),
    }
}

#[get("/api/tenant")]
pub async fn get_tenant(
    user: User,
    pool: &State<PgPool>,
    actions: &State<ActionSet>,
    chipotle: &State<ChipotleClient>,
) -> ApiResult<TenantResponse> {
    let t = find_by_user(pool, user.id)
        .await
        .map_err(|e| internal("tenant_lookup_failed", e))?;
    Ok(Json(tenant_response(t.as_ref(), actions, chipotle)))
}

#[post("/api/tenant/provision")]
pub async fn provision_tenant(
    user: User,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    chipotle: &State<ChipotleClient>,
    actions: &State<ActionSet>,
    lock: &State<ProvisionLock>,
) -> ApiResult<TenantResponse> {
    let t = ensure_tenant(pool, cfg, chipotle, actions, lock, user.id).await?;
    Ok(Json(tenant_response(Some(&t), actions, chipotle)))
}

#[derive(Debug, Deserialize)]
pub struct AddTenantActionRequest {
    pub cid: String,
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TenantActionResponse {
    pub id: Uuid,
    pub cid: String,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

fn validate_cid(cid: &str) -> Result<(), ApiError> {
    let ok = (32..=128).contains(&cid.len()) && cid.bytes().all(|b| b.is_ascii_alphanumeric());
    if ok {
        Ok(())
    } else {
        Err(err_detail(
            Status::BadRequest,
            "invalid_cid",
            "expected a base58/base32 IPFS CID",
        ))
    }
}

/// Attach a customer action to the tenant group so it can decrypt vault
/// ciphertexts in-TEE (the `in_tee_only` release tier).
#[post("/api/actions", data = "<req>")]
pub async fn add_tenant_action(
    user: User,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    chipotle: &State<ChipotleClient>,
    actions: &State<ActionSet>,
    lock: &State<ProvisionLock>,
    req: Json<AddTenantActionRequest>,
) -> ApiResult<TenantActionResponse> {
    let req = req.into_inner();
    let cid = req.cid.trim().to_string();
    validate_cid(&cid)?;
    let name = req
        .name
        .map(|n| n.trim().chars().take(128).collect::<String>())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| cid.clone());

    let tenant = ensure_tenant(pool, cfg, chipotle, actions, lock, user.id).await?;
    ensure_action_registered(
        pool,
        cfg,
        chipotle,
        &cid,
        &name,
        "tenant action (lit-secrets)",
    )
    .await?;
    chipotle
        .add_action_to_group(&cfg.chipotle_master_api_key, tenant.group_id_u64(), &cid)
        .await
        .map_err(|e| upstream("chipotle_add_action_to_group_failed", &e))?;

    let row = sqlx::query(
        "INSERT INTO tenant_actions (id, tenant_id, cid, name) VALUES ($1, $2, $3, $4)
         ON CONFLICT (tenant_id, cid) DO UPDATE SET name = EXCLUDED.name
         RETURNING id, cid, name, created_at",
    )
    .bind(Uuid::new_v4())
    .bind(tenant.id)
    .bind(&cid)
    .bind(&name)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| internal("tenant_action_insert_failed", e))?;
    Ok(Json(TenantActionResponse {
        id: row.get("id"),
        cid: row.get("cid"),
        name: row.get("name"),
        created_at: row.get("created_at"),
    }))
}

#[get("/api/actions")]
pub async fn list_tenant_actions(
    user: User,
    pool: &State<PgPool>,
) -> ApiResult<Vec<TenantActionResponse>> {
    let Some(tenant) = find_by_user(pool, user.id)
        .await
        .map_err(|e| internal("tenant_lookup_failed", e))?
    else {
        return Ok(Json(vec![]));
    };
    let rows = sqlx::query(
        "SELECT id, cid, name, created_at FROM tenant_actions WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(tenant.id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| internal("tenant_actions_list_failed", e))?;
    Ok(Json(
        rows.into_iter()
            .map(|r| TenantActionResponse {
                id: r.get("id"),
                cid: r.get("cid"),
                name: r.get("name"),
                created_at: r.get("created_at"),
            })
            .collect(),
    ))
}

#[delete("/api/actions/<id>")]
pub async fn remove_tenant_action(
    user: User,
    pool: &State<PgPool>,
    cfg: &State<Config>,
    chipotle: &State<ChipotleClient>,
    id: &str,
) -> Result<Status, ApiError> {
    let id = Uuid::parse_str(id).map_err(|_| err(Status::BadRequest, "invalid_id"))?;
    let Some(tenant) = find_by_user(pool, user.id)
        .await
        .map_err(|e| internal("tenant_lookup_failed", e))?
    else {
        return Err(err(Status::NotFound, "not_found"));
    };
    let Some((cid,)) = sqlx::query_as::<_, (String,)>(
        "SELECT cid FROM tenant_actions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant.id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| internal("tenant_action_lookup_failed", e))?
    else {
        return Err(err(Status::NotFound, "not_found"));
    };
    chipotle
        .remove_action_from_group(
            &cfg.chipotle_master_api_key,
            tenant.group_id_u64(),
            &hashed_cid(&cid),
        )
        .await
        .map_err(|e| upstream("chipotle_remove_action_from_group_failed", &e))?;
    sqlx::query("DELETE FROM tenant_actions WHERE id = $1")
        .bind(id)
        .execute(pool.inner())
        .await
        .map_err(|e| internal("tenant_action_delete_failed", e))?;
    Ok(Status::NoContent)
}
