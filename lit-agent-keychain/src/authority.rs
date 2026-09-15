//! Authority release versions per vault. Sign-ins may come from any archived
//! authority release; each release the vault has used is granted to its fixed
//! Chipotle group so owners can keep operating secrets created under it.
use crate::{
    actions,
    api::{self, ApiError},
    chipotle::Chipotle,
    models::Authority,
};
use rocket::http::Status;
use sqlx::{PgPool, Postgres, Transaction};

fn unavailable(_: impl std::fmt::Display) -> ApiError {
    api::err(Status::BadGateway, "lit_unavailable")
}
pub async fn vault_authority(pool: &PgPool, vault: &str) -> Result<Authority, ApiError> {
    let value: serde_json::Value =
        sqlx::query_scalar("SELECT authority FROM kc_vaults WHERE id=$1")
            .bind(vault)
            .fetch_one(pool)
            .await
            .map_err(api::internal)?;
    serde_json::from_value(value).map_err(api::internal)
}
/// Verifies `cid` is a released authority for this vault and returns its action key.
pub async fn key_for(
    pool: &PgPool,
    lit: &Chipotle,
    vault: &str,
    cid: &str,
) -> Result<String, ApiError> {
    let authority = vault_authority(pool, vault).await?;
    let versions = actions::authority_versions(&authority).map_err(api::internal)?;
    if !versions.iter().any(|v| v.cid == cid) {
        return Err(api::err(Status::Forbidden, "wrong_authority"));
    }
    lit.public_key(cid).await.map_err(unavailable)
}
/// Finds the vault authority release that signed `signed`, newest first.
pub async fn verify_any(
    pool: &PgPool,
    lit: &Chipotle,
    vault: &str,
    signed: &crate::models::Signed,
) -> Result<(String, String), ApiError> {
    let authority = vault_authority(pool, vault).await?;
    verify_with(lit, &authority, vault, signed).await
}
pub async fn verify_with(
    lit: &Chipotle,
    authority: &Authority,
    vault: &str,
    signed: &crate::models::Signed,
) -> Result<(String, String), ApiError> {
    for version in actions::authority_versions(authority).map_err(api::invalid)? {
        let key = lit.public_key(&version.cid).await.map_err(unavailable)?;
        if crate::crypto::verify_signed(signed, &key, vault).is_ok() {
            return Ok((version.cid, key));
        }
    }
    Err(api::denied("no authority release verifies this receipt"))
}
/// Records that `cid` is in use for the vault, grants it to the vault's fixed
/// Chipotle group on first use, and moves the vault's current authority forward
/// when `cid` is a newer release than the one recorded.
pub async fn ensure_granted(
    tx: &mut Transaction<'_, Postgres>,
    lit: &Chipotle,
    vault: &str,
    cid: &str,
) -> Result<(), ApiError> {
    let inserted = sqlx::query(
        "INSERT INTO kc_vault_authorities(vault_id,authority_cid) VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(vault)
    .bind(cid)
    .execute(&mut **tx)
    .await
    .map_err(api::internal)?
    .rows_affected();
    if inserted == 0 {
        return Ok(());
    }
    let group: Option<i64> =
        sqlx::query_scalar("SELECT group_id FROM kc_execution_accounts WHERE vault_id=$1")
            .bind(vault)
            .fetch_optional(&mut **tx)
            .await
            .map_err(api::internal)?;
    if let Some(group) = group {
        lit.add_action(group, cid).await.map_err(unavailable)?;
    }
    let (authority, current): (serde_json::Value, String) =
        sqlx::query_as("SELECT authority,authority_cid FROM kc_vaults WHERE id=$1")
            .bind(vault)
            .fetch_one(&mut **tx)
            .await
            .map_err(api::internal)?;
    let authority: Authority = serde_json::from_value(authority).map_err(api::internal)?;
    let versions = actions::authority_versions(&authority).map_err(api::internal)?;
    let index = |c: &str| versions.iter().position(|v| v.cid == c);
    if index(cid) < index(&current) || index(&current).is_none() {
        sqlx::query("UPDATE kc_vaults SET authority_cid=$2 WHERE id=$1")
            .bind(vault)
            .bind(cid)
            .execute(&mut **tx)
            .await
            .map_err(api::internal)?;
        sqlx::query(
            "INSERT INTO kc_audit(vault_id,event,object_hash) VALUES($1,'authority_upgraded',$2)",
        )
        .bind(vault)
        .bind(cid)
        .execute(&mut **tx)
        .await
        .map_err(api::internal)?;
    }
    Ok(())
}
