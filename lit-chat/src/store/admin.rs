//! Admin roster, passkey credentials, and the hash-chained audit log
//! (plans/tee-chat-app.md section 4.4).

use crate::crypto::{aes, constant_time_eq, hmac_sha256, keccak256};
use anyhow::Result;
use sqlx::PgPool;

// ---------------------------------------------------------------------------
// Roster — rows are enclave-MAC'd; a bare DB row is never the root of trust.

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AdminRow {
    pub user_ref_hash: String,
    pub role: String,
    pub granted_by: String,
    pub granted_at_unix: i64,
    pub mac: Vec<u8>,
}

fn roster_mac(key: &[u8; 32], row: &AdminRow) -> [u8; 32] {
    let msg = format!(
        "chat.admin.v1|{}|{}|{}|{}",
        row.user_ref_hash, row.role, row.granted_by, row.granted_at_unix
    );
    hmac_sha256(key, msg.as_bytes())
}

pub fn roster_row_valid(key: &[u8; 32], row: &AdminRow) -> bool {
    constant_time_eq(&roster_mac(key, row), &row.mac)
}

pub async fn grant(
    pool: &PgPool,
    mac_key: &[u8; 32],
    user_ref_hash: &str,
    granted_by: &str,
) -> Result<()> {
    let mut row = AdminRow {
        user_ref_hash: user_ref_hash.to_string(),
        role: "admin".to_string(),
        granted_by: granted_by.to_string(),
        granted_at_unix: time::OffsetDateTime::now_utc().unix_timestamp(),
        mac: Vec::new(),
    };
    row.mac = roster_mac(mac_key, &row).to_vec();
    sqlx::query(
        "INSERT INTO admins (user_ref_hash, role, granted_by, granted_at_unix, mac)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (user_ref_hash) DO NOTHING",
    )
    .bind(&row.user_ref_hash)
    .bind(&row.role)
    .bind(&row.granted_by)
    .bind(row.granted_at_unix)
    .bind(&row.mac)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn revoke(pool: &PgPool, user_ref_hash: &str) -> Result<bool> {
    let res = sqlx::query("DELETE FROM admins WHERE user_ref_hash = $1")
        .bind(user_ref_hash)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// Roster membership check: the row must exist AND carry a valid enclave MAC.
/// An operator-inserted row without the MAC is ignored.
pub async fn is_admin(pool: &PgPool, mac_key: &[u8; 32], user_ref_hash: &str) -> Result<bool> {
    let row: Option<AdminRow> = sqlx::query_as(
        "SELECT user_ref_hash, role, granted_by, granted_at_unix, mac
         FROM admins WHERE user_ref_hash = $1",
    )
    .bind(user_ref_hash)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) if roster_row_valid(mac_key, &r) => Ok(true),
        Some(_) => {
            tracing::warn!(
                user_ref_hash,
                "admins row with INVALID MAC ignored (possible DB-operator tampering)"
            );
            Ok(false)
        }
        None => Ok(false),
    }
}

pub async fn list_admins(pool: &PgPool, mac_key: &[u8; 32]) -> Result<Vec<(AdminRow, bool)>> {
    let rows: Vec<AdminRow> = sqlx::query_as(
        "SELECT user_ref_hash, role, granted_by, granted_at_unix, mac
         FROM admins ORDER BY granted_at_unix ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let valid = roster_row_valid(mac_key, &r);
            (r, valid)
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Passkey credentials — ciphertext under the service cred KEK; AAD binds the
// credential id to its owner so rows cannot be re-pointed.

fn cred_aad(cred_id: &str, user_ref_hash: &str) -> String {
    format!("chat.cred.v1|{cred_id}|{user_ref_hash}")
}

pub async fn store_credential(
    pool: &PgPool,
    cred_kek: &[u8; 32],
    cred_id: &str,
    user_ref_hash: &str,
    passkey_json: &[u8],
) -> Result<()> {
    let enc = aes::encrypt(
        cred_kek,
        passkey_json,
        cred_aad(cred_id, user_ref_hash).as_bytes(),
    )?;
    sqlx::query(
        "INSERT INTO admin_credentials (cred_id, user_ref_hash, enc_passkey) VALUES ($1, $2, $3)
         ON CONFLICT (cred_id) DO NOTHING",
    )
    .bind(cred_id)
    .bind(user_ref_hash)
    .bind(enc)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_credentials(
    pool: &PgPool,
    cred_kek: &[u8; 32],
    user_ref_hash: &str,
) -> Result<Vec<Vec<u8>>> {
    let rows: Vec<(String, Vec<u8>)> = sqlx::query_as(
        "SELECT cred_id, enc_passkey FROM admin_credentials WHERE user_ref_hash = $1",
    )
    .bind(user_ref_hash)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (cred_id, enc) in rows {
        out.push(aes::decrypt(
            cred_kek,
            &enc,
            cred_aad(&cred_id, user_ref_hash).as_bytes(),
        )?);
    }
    Ok(out)
}

pub async fn credential_count(pool: &PgPool, user_ref_hash: &str) -> Result<i64> {
    let row: (i64,) =
        sqlx::query_as("SELECT count(*) FROM admin_credentials WHERE user_ref_hash = $1")
            .bind(user_ref_hash)
            .fetch_one(pool)
            .await?;
    Ok(row.0)
}

// ---------------------------------------------------------------------------
// Audit log — append-only, hash-chained, TEE-MAC'd. Chain makes edits
// evident; deletions of the tail remain possible for a DB operator
// (disclosed in section 4.4).

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditRow {
    pub id: i64,
    pub actor_ref_hash: String,
    pub action: String,
    pub subject: String,
    pub detail: String,
    pub prev_hash: Vec<u8>,
    pub row_hash: Vec<u8>,
    pub mac: Vec<u8>,
    pub created_at_unix: i64,
}

fn audit_row_hash(
    prev_hash: &[u8],
    actor: &str,
    action: &str,
    subject: &str,
    detail: &str,
    created_at_unix: i64,
) -> [u8; 32] {
    let mut preimage = Vec::new();
    preimage.extend_from_slice(prev_hash);
    preimage.extend_from_slice(
        format!("|chat.audit.v1|{actor}|{action}|{subject}|{detail}|{created_at_unix}").as_bytes(),
    );
    keccak256(&preimage)
}

/// Append an audit row. `detail` must contain masked hints only — callers
/// are responsible for never passing key material.
pub async fn audit(
    pool: &PgPool,
    audit_mac_key: &[u8; 32],
    actor_ref_hash: &str,
    action: &str,
    subject: &str,
    detail: &str,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let prev: Option<(Vec<u8>,)> =
        sqlx::query_as("SELECT row_hash FROM admin_audit_log ORDER BY id DESC LIMIT 1 FOR UPDATE")
            .fetch_optional(&mut *tx)
            .await?;
    let prev_hash = prev.map(|(h,)| h).unwrap_or_else(|| vec![0u8; 32]);
    let created_at_unix = time::OffsetDateTime::now_utc().unix_timestamp();
    let row_hash = audit_row_hash(
        &prev_hash,
        actor_ref_hash,
        action,
        subject,
        detail,
        created_at_unix,
    );
    let mac = hmac_sha256(audit_mac_key, &row_hash);
    sqlx::query(
        "INSERT INTO admin_audit_log
           (actor_ref_hash, action, subject, detail, prev_hash, row_hash, mac, created_at_unix)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(actor_ref_hash)
    .bind(action)
    .bind(subject)
    .bind(detail)
    .bind(&prev_hash)
    .bind(row_hash.to_vec())
    .bind(mac.to_vec())
    .bind(created_at_unix)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// List recent audit rows, verifying each row's MAC and its chain link.
/// Returns (row, mac_valid, chain_valid).
pub async fn audit_list(
    pool: &PgPool,
    audit_mac_key: &[u8; 32],
    limit: i64,
) -> Result<Vec<(AuditRow, bool, bool)>> {
    let rows: Vec<AuditRow> = sqlx::query_as(
        "SELECT id, actor_ref_hash, action, subject, detail, prev_hash, row_hash, mac,
                created_at_unix
         FROM admin_audit_log ORDER BY id DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let recomputed = audit_row_hash(
                &r.prev_hash,
                &r.actor_ref_hash,
                &r.action,
                &r.subject,
                &r.detail,
                r.created_at_unix,
            );
            let chain_valid = constant_time_eq(&recomputed, &r.row_hash);
            let mac_valid = constant_time_eq(&hmac_sha256(audit_mac_key, &r.row_hash), &r.mac);
            (r, mac_valid, chain_valid)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_mac_detects_tampering() {
        let key = [4u8; 32];
        let mut row = AdminRow {
            user_ref_hash: "abc".into(),
            role: "admin".into(),
            granted_by: "bootstrap".into(),
            granted_at_unix: 1_700_000_000,
            mac: Vec::new(),
        };
        row.mac = roster_mac(&key, &row).to_vec();
        assert!(roster_row_valid(&key, &row));
        // An operator inserting/modifying a row cannot produce a valid MAC.
        row.user_ref_hash = "attacker".into();
        assert!(!roster_row_valid(&key, &row));
    }

    #[test]
    fn audit_chain_detects_edit() {
        let prev = [0u8; 32];
        let h1 = audit_row_hash(&prev, "a", "key.import", "k1", "hint", 100);
        let h1_edited = audit_row_hash(&prev, "a", "key.import", "k2", "hint", 100);
        assert_ne!(h1, h1_edited);
    }
}
