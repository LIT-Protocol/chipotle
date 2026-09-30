//! Device-style agent pairing.
//!
//! Flow:
//! 1. The agent generates a random token `T` locally, computes `H = token_hash(T)`,
//!    and calls `POST /agent/pair` with `H` ([`start`]). The server issues an
//!    opaque single-use `code` (carried in the authorize URL) plus a short
//!    `user_code` the agent shows the user.
//! 2. The signed-in user opens the authorize URL, confirms the `user_code`
//!    matches what their agent displayed, and approves ([`approve`]).
//! 3. The agent polls [`complete`] presenting `Bearer T`. The server verifies the
//!    pairing was approved and that `token_hash(T)` matches the `H` recorded at
//!    step 1 (proof of possession), then binds the token to the approving user.
//!
//! The binding target (`H`) is fixed at step 1 by the agent and stored
//! server-side; the browser never supplies it. The `code` is a server-issued,
//! single-use, short-lived secret, so an attacker cannot make a victim bind an
//! arbitrary hash by poisoning the post-login redirect.

use anyhow::{Context, Result};
use base64::Engine;
use rand::RngCore;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use super::agent;
use super::token as auth_token;

/// Pairings expire quickly: they only need to survive one human sign-in +
/// approval round-trip.
pub const PAIRING_TTL_SECONDS: i64 = 10 * 60;

/// Hard cap on concurrent pending pairings. `POST /agent/pair` is unauthenticated
/// and inserts a row, so without a bound an attacker could grow the table without
/// limit between restarts. Legit concurrent pairings across all users are a
/// handful, so this ceiling is far above normal load while still bounding a flood.
/// Enforced in the DB (see [`start`]) so it holds regardless of client IP / proxy
/// configuration, and needs no in-memory per-source state to grow.
const MAX_PENDING_PAIRINGS: i64 = 10_000;

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Length of the opaque pairing code (base64url of 32 random bytes, no padding).
const PAIRING_CODE_LEN: usize = 43;

pub struct StartedPairing {
    pub code: String,
    pub user_code: String,
    pub expires_in_seconds: i64,
}

pub struct PairingDisplay {
    pub label: String,
    pub user_code: String,
}

/// Outcome of an agent's [`complete`] poll.
pub enum CompleteOutcome {
    /// User has not approved yet; keep polling.
    Pending,
    /// Approved and bound — the token is now usable.
    Authorized,
    /// No matching pending pairing (unknown code, expired, or already consumed).
    NotFound,
    /// The presented token does not match the hash registered for this code.
    Mismatch,
}

fn generate_code() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    B64.encode(bytes)
}

/// Human-friendly confirmation code, e.g. `4F9A-2B7C`. Not a secret; it exists so
/// the user can visually confirm the pairing matches their agent's.
fn generate_user_code() -> String {
    let mut bytes = [0u8; 4];
    rand::thread_rng().fill_bytes(&mut bytes);
    let hex = hex::encode_upper(bytes);
    format!("{}-{}", &hex[0..4], &hex[4..8])
}

pub fn validate_code(code: &str) -> Result<()> {
    if code.len() != PAIRING_CODE_LEN {
        anyhow::bail!("pairing code has invalid length");
    }
    if !code
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        anyhow::bail!("pairing code contains unsupported characters");
    }
    Ok(())
}

/// Register an agent token hash and mint a single-use pairing code.
///
/// Returns `Ok(None)` when the pending-pairing cap is reached (caller should
/// respond with a retry-later status), `Err` on validation failure.
pub async fn start(
    pool: &PgPool,
    token_hash: &str,
    label: Option<&str>,
) -> Result<Option<StartedPairing>> {
    agent::validate_agent_token_hash(token_hash)?;
    let label = label.map(str::trim).filter(|s| !s.is_empty());
    // Cap length so an unauthenticated caller can't store an oversized label
    // (row count is already bounded by MAX_PENDING_PAIRINGS).
    let label = label.map(|s| &s[..s.char_indices().nth(64).map_or(s.len(), |(i, _)| i)]);
    let code = generate_code();
    let user_code = generate_user_code();
    let expires_at = OffsetDateTime::now_utc() + time::Duration::seconds(PAIRING_TTL_SECONDS);

    // Insert only while under the pending cap. The count is evaluated inside the
    // statement so the bound holds under concurrency without a separate query.
    let inserted = sqlx::query(
        "INSERT INTO agent_pairings (code_hash, token_hash, user_code, label, expires_at)
         SELECT $1, $2, $3, $4, $5
         WHERE (
             SELECT count(*) FROM agent_pairings
             WHERE consumed_at IS NULL AND expires_at > now()
         ) < $6",
    )
    .bind(auth_token::token_hash(&code))
    .bind(token_hash)
    .bind(&user_code)
    .bind(label)
    .bind(expires_at)
    .bind(MAX_PENDING_PAIRINGS)
    .execute(pool)
    .await?;

    if inserted.rows_affected() == 0 {
        return Ok(None);
    }

    Ok(Some(StartedPairing {
        code,
        user_code,
        expires_in_seconds: PAIRING_TTL_SECONDS,
    }))
}

/// Fetch the label + user_code for a still-pending pairing so the browser can
/// show the user what they are approving.
pub async fn display_info(pool: &PgPool, code: &str) -> Result<Option<PairingDisplay>> {
    validate_code(code).context("invalid pairing code")?;
    let row = sqlx::query_as::<_, (Option<String>, String)>(
        "SELECT label, user_code FROM agent_pairings
         WHERE code_hash = $1 AND consumed_at IS NULL AND expires_at > now()",
    )
    .bind(auth_token::token_hash(code))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(label, user_code)| PairingDisplay {
        label: label.unwrap_or_else(|| "local-agent".to_string()),
        user_code,
    }))
}

/// Mark a pending pairing approved by `user_id`. Idempotent for the same user.
/// Returns `true` if an approvable pairing was found.
pub async fn approve(pool: &PgPool, code: &str, user_id: Uuid) -> Result<bool> {
    validate_code(code).context("invalid pairing code")?;
    let row = sqlx::query_as::<_, (Uuid,)>(
        "UPDATE agent_pairings
         SET approved_user_id = $2, approved_at = now()
         WHERE code_hash = $1
           AND consumed_at IS NULL
           AND expires_at > now()
           AND (approved_user_id IS NULL OR approved_user_id = $2)
         RETURNING approved_user_id",
    )
    .bind(auth_token::token_hash(code))
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

/// Agent-side completion: proves possession of the raw token preimage and, if the
/// pairing was approved, binds the token to the approving user and consumes the
/// pairing (single-use).
pub async fn complete(pool: &PgPool, code: &str, raw_token: &str) -> Result<CompleteOutcome> {
    validate_code(code).context("invalid pairing code")?;
    agent::validate_agent_token(raw_token).context("invalid agent bearer token")?;
    let presented_hash = auth_token::token_hash(raw_token);

    let row = sqlx::query_as::<_, (String, Option<Uuid>, Option<String>)>(
        "SELECT token_hash, approved_user_id, label FROM agent_pairings
         WHERE code_hash = $1 AND consumed_at IS NULL AND expires_at > now()",
    )
    .bind(auth_token::token_hash(code))
    .fetch_optional(pool)
    .await?;

    let Some((token_hash, approved_user_id, label)) = row else {
        return Ok(CompleteOutcome::NotFound);
    };

    // Proof of possession: the caller must hold the preimage of the hash that was
    // registered when the pairing was created.
    if token_hash != presented_hash {
        return Ok(CompleteOutcome::Mismatch);
    }

    let Some(user_id) = approved_user_id else {
        return Ok(CompleteOutcome::Pending);
    };

    // Atomically claim the pairing so a concurrent poll can't double-bind.
    let claimed = sqlx::query(
        "UPDATE agent_pairings SET consumed_at = now()
         WHERE code_hash = $1 AND consumed_at IS NULL",
    )
    .bind(auth_token::token_hash(code))
    .execute(pool)
    .await?;
    if claimed.rows_affected() == 0 {
        // Someone else consumed it between the SELECT and here. The token is
        // already (being) bound, so treat as authorized.
        return Ok(CompleteOutcome::Authorized);
    }

    let bound = agent::bind_token(pool, &token_hash, user_id, label.as_deref()).await?;
    if bound == 0 {
        // The hash is already owned by a different user, so the guarded upsert
        // was a no-op. Don't claim success — the token is not bound to this user.
        return Ok(CompleteOutcome::Mismatch);
    }
    Ok(CompleteOutcome::Authorized)
}

/// Delete expired/consumed pairings. Best-effort housekeeping.
pub async fn purge_expired(pool: &PgPool) -> Result<u64> {
    let r = sqlx::query(
        "DELETE FROM agent_pairings WHERE expires_at <= now() OR consumed_at IS NOT NULL",
    )
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_code_passes_validation() {
        for _ in 0..50 {
            assert!(validate_code(&generate_code()).is_ok());
        }
    }

    #[test]
    fn user_code_is_formatted() {
        let uc = generate_user_code();
        assert_eq!(uc.len(), 9);
        assert_eq!(uc.as_bytes()[4], b'-');
    }

    #[test]
    fn rejects_malformed_codes() {
        assert!(validate_code("short").is_err());
        let bad = format!("{}+", "A".repeat(PAIRING_CODE_LEN - 1));
        assert!(validate_code(&bad).is_err());
    }
}
