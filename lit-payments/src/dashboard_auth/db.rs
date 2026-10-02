//! Every SQL statement of the dashboard auth service. The conditional
//! `UPDATE … WHERE state = … AND version = … AND verifier = …` shapes are the
//! signup state machine: a write only lands when the row is still in the
//! state the caller authenticated against.

use anyhow::Result;
use sqlx::{PgExecutor, PgPool};

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub email: String,
    pub salt: String,
    pub verifier: Option<String>,
    pub version: i64,
    pub state: String,
    pub account: Option<String>,
    pub envelope: Option<String>,
    pub operation: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Session {
    pub hash: String,
    pub user_id: String,
    pub version: i64,
    pub csrf: String,
    pub scope: String,
    pub expires_at: i64,
    pub idle_until: i64,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct OutboxRow {
    pub id: String,
    pub payload: String,
    pub attempts: i64,
}

pub async fn user_by_id(executor: impl PgExecutor<'_>, id: &str) -> Result<Option<User>> {
    Ok(
        sqlx::query_as::<_, User>("SELECT * FROM dashboard_auth_users WHERE id = $1")
            .bind(id)
            .fetch_optional(executor)
            .await?,
    )
}

pub async fn user_by_email(executor: impl PgExecutor<'_>, email: &str) -> Result<Option<User>> {
    Ok(
        sqlx::query_as::<_, User>("SELECT * FROM dashboard_auth_users WHERE email = $1")
            .bind(email)
            .fetch_optional(executor)
            .await?,
    )
}

pub async fn credentialed_user_by_email(
    executor: impl PgExecutor<'_>,
    email: &str,
) -> Result<Option<User>> {
    Ok(sqlx::query_as::<_, User>(
        "SELECT * FROM dashboard_auth_users WHERE email = $1 AND verifier IS NOT NULL",
    )
    .bind(email)
    .fetch_optional(executor)
    .await?)
}

pub async fn email_taken(executor: impl PgExecutor<'_>, email: &str) -> Result<bool> {
    Ok(
        sqlx::query_scalar::<_, i32>("SELECT 1 FROM dashboard_auth_users WHERE email = $1")
            .bind(email)
            .fetch_optional(executor)
            .await?
            .is_some(),
    )
}

/// Conditional rate-limit counter. Returns the count for this window.
pub async fn bump_limit(executor: impl PgExecutor<'_>, key: &str, expires_at: i64) -> Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "INSERT INTO dashboard_auth_limits (key, count, expires_at) VALUES ($1, 1, $2) \
         ON CONFLICT (key) DO UPDATE SET count = dashboard_auth_limits.count + 1 \
         RETURNING count",
    )
    .bind(key)
    .bind(expires_at)
    .fetch_one(executor)
    .await?)
}

pub async fn live_session(
    executor: impl PgExecutor<'_>,
    hash: &str,
    now: i64,
) -> Result<Option<Session>> {
    Ok(sqlx::query_as::<_, Session>(
        "SELECT * FROM dashboard_auth_sessions WHERE hash = $1 AND expires_at > $2 AND idle_until > $2",
    )
    .bind(hash)
    .bind(now)
    .fetch_optional(executor)
    .await?)
}

pub async fn touch_session(
    executor: impl PgExecutor<'_>,
    hash: &str,
    idle_until: i64,
) -> Result<()> {
    sqlx::query("UPDATE dashboard_auth_sessions SET idle_until = $1 WHERE hash = $2")
        .bind(idle_until)
        .bind(hash)
        .execute(executor)
        .await?;
    Ok(())
}

/// Insert a session only if the user row still has the version and verifier
/// the caller authenticated against (closes the login/password-change race).
/// Returns `false` when nothing was inserted.
#[allow(clippy::too_many_arguments)]
pub async fn insert_session_if_current(
    executor: impl PgExecutor<'_>,
    hash: &str,
    csrf: &str,
    scope: &str,
    expires_at: i64,
    idle_until: i64,
    user_id: &str,
    version: i64,
    verifier: Option<&str>,
) -> Result<bool> {
    let result = sqlx::query(
        "INSERT INTO dashboard_auth_sessions (hash, user_id, version, csrf, scope, expires_at, idle_until) \
         SELECT $1, id, version, $2, $3, $4, $5 FROM dashboard_auth_users \
         WHERE id = $6 AND version = $7 AND verifier IS NOT DISTINCT FROM $8",
    )
    .bind(hash)
    .bind(csrf)
    .bind(scope)
    .bind(expires_at)
    .bind(idle_until)
    .bind(user_id)
    .bind(version)
    .bind(verifier)
    .execute(executor)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn delete_session(executor: impl PgExecutor<'_>, hash: &str) -> Result<()> {
    sqlx::query("DELETE FROM dashboard_auth_sessions WHERE hash = $1")
        .bind(hash)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn delete_user_sessions(executor: impl PgExecutor<'_>, user_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM dashboard_auth_sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(executor)
        .await?;
    Ok(())
}

/// Sessions and email proofs issued for credential versions up to `version`.
pub async fn delete_stale_credentials(
    executor: &mut sqlx::PgConnection,
    user_id: &str,
    version: i64,
) -> Result<()> {
    sqlx::query("DELETE FROM dashboard_auth_sessions WHERE user_id = $1 AND version <= $2")
        .bind(user_id)
        .bind(version)
        .execute(&mut *executor)
        .await?;
    sqlx::query("DELETE FROM dashboard_auth_tokens WHERE user_id = $1 AND version <= $2")
        .bind(user_id)
        .bind(version)
        .execute(&mut *executor)
        .await?;
    Ok(())
}

pub async fn insert_token(
    executor: impl PgExecutor<'_>,
    hash: &str,
    email: &str,
    user_id: Option<&str>,
    version: Option<i64>,
    purpose: &str,
    expires_at: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO dashboard_auth_tokens (hash, email, user_id, version, purpose, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(hash)
    .bind(email)
    .bind(user_id)
    .bind(version)
    .bind(purpose)
    .bind(expires_at)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn insert_outbox(
    executor: impl PgExecutor<'_>,
    id: &str,
    payload: &str,
    next_at: i64,
    expires_at: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO dashboard_auth_outbox (id, payload, next_at, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(payload)
    .bind(next_at)
    .bind(expires_at)
    .execute(executor)
    .await?;
    Ok(())
}

/// Signup verification, in one transaction: claim the unused signup token,
/// create the user row if the email is new, and open a signup-scoped session
/// only when the user has not chosen a password yet.
#[allow(clippy::too_many_arguments)]
pub async fn verify_signup(
    pool: &PgPool,
    token_hash: &str,
    claim: &str,
    user_id: &str,
    salt: &str,
    csrf: &str,
    now: i64,
    session_expires_at: i64,
    session_idle_until: i64,
) -> Result<Option<User>> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE dashboard_auth_tokens SET claim = $1 \
         WHERE hash = $2 AND purpose = 'signup' AND claim IS NULL AND expires_at > $3",
    )
    .bind(claim)
    .bind(token_hash)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO dashboard_auth_users (id, email, salt, created_at) \
         SELECT $1, email, $2, $3 FROM dashboard_auth_tokens WHERE hash = $4 AND claim = $5 \
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(salt)
    .bind(now)
    .bind(token_hash)
    .bind(claim)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO dashboard_auth_sessions (hash, user_id, version, csrf, scope, expires_at, idle_until) \
         SELECT $1, u.id, u.version, $2, 'signup', $3, $4 \
         FROM dashboard_auth_users u JOIN dashboard_auth_tokens t ON t.email = u.email \
         WHERE t.hash = $5 AND t.claim = $6 AND u.state = 'verified' AND u.verifier IS NULL",
    )
    .bind(claim)
    .bind(csrf)
    .bind(session_expires_at)
    .bind(session_idle_until)
    .bind(token_hash)
    .bind(claim)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(sqlx::query_as::<_, User>(
        "SELECT u.* FROM dashboard_auth_users u \
         JOIN dashboard_auth_sessions s ON s.user_id = u.id WHERE s.hash = $1",
    )
    .bind(claim)
    .fetch_optional(pool)
    .await?)
}

pub async fn set_credentials(
    executor: impl PgExecutor<'_>,
    user_id: &str,
    version: i64,
    verifier: &str,
    operation: &str,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE dashboard_auth_users SET verifier = $1, state = 'reserved', operation = $2 \
         WHERE id = $3 AND state = 'verified' AND verifier IS NULL AND version = $4",
    )
    .bind(verifier)
    .bind(operation)
    .bind(user_id)
    .bind(version)
    .execute(executor)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn claim_creation(executor: impl PgExecutor<'_>, u: &User) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE dashboard_auth_users SET state = 'creating' \
         WHERE id = $1 AND state = 'reserved' AND version = $2 AND verifier = $3",
    )
    .bind(&u.id)
    .bind(u.version)
    .bind(&u.verifier)
    .execute(executor)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn release_creation(
    executor: impl PgExecutor<'_>,
    u: &User,
    operation: &str,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE dashboard_auth_users SET operation = $1 \
         WHERE id = $2 AND state = 'creating' AND operation = $3 AND version = $4 AND verifier = $5",
    )
    .bind(operation)
    .bind(&u.id)
    .bind(&u.operation)
    .bind(u.version)
    .bind(&u.verifier)
    .execute(executor)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn activate(
    executor: impl PgExecutor<'_>,
    u: &User,
    account: &str,
    envelope: &str,
    operation: &str,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE dashboard_auth_users SET account = $1, envelope = $2, state = 'active' \
         WHERE id = $3 AND state = 'creating' AND version = $4 AND verifier = $5 AND operation = $6",
    )
    .bind(account)
    .bind(envelope)
    .bind(&u.id)
    .bind(u.version)
    .bind(&u.verifier)
    .bind(operation)
    .execute(executor)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Rotate the credential (verifier, salt, envelope, version+1) and queue the
/// notification in the same transaction. `false` if the row changed under us.
pub async fn change_password(
    pool: &PgPool,
    u: &User,
    verifier: &str,
    salt: &str,
    envelope: &str,
    notice: &super::outbox::Queued,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        "UPDATE dashboard_auth_users SET verifier = $1, salt = $2, envelope = $3, version = version + 1 \
         WHERE id = $4 AND version = $5 AND verifier = $6",
    )
    .bind(verifier)
    .bind(salt)
    .bind(envelope)
    .bind(&u.id)
    .bind(u.version)
    .bind(&u.verifier)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() != 1 {
        tx.rollback().await?;
        return Ok(false);
    }
    insert_outbox(
        &mut *tx,
        &notice.id,
        &notice.payload,
        notice.next_at,
        notice.expires_at,
    )
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// Is the address an `email` proof points to already registered by another user?
pub async fn email_proof_taken(pool: &PgPool, user_id: &str, token_hash: &str) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM dashboard_auth_users WHERE id <> $1 AND email = \
         (SELECT email FROM dashboard_auth_tokens WHERE hash = $2 AND user_id = $1 AND purpose = 'email')",
    )
    .bind(user_id)
    .bind(token_hash)
    .fetch_optional(pool)
    .await?
    .is_some())
}

/// Email change, in one transaction: consume the proof (only if the user's
/// credential is unchanged), move the address, queue the notice to the old
/// address, and revoke this user's sessions and other proofs. The version is
/// deliberately not incremented: the email is outside the envelope AAD.
/// `false` when the proof was expired, used, or not this user's.
pub async fn complete_email_change(
    pool: &PgPool,
    u: &User,
    token_hash: &str,
    claim: &str,
    now: i64,
    notice: &super::outbox::Queued,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    // Lock the user row with the credential the caller authenticated against.
    // Under READ COMMITTED a concurrent password change could otherwise commit
    // between the proof check and the email write; the lock serialises the
    // two and the re-check rejects a rotated credential. (The D1 batch was
    // serialised implicitly.)
    let current = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM dashboard_auth_users WHERE id = $1 AND version = $2 AND verifier = $3 FOR UPDATE",
    )
    .bind(&u.id)
    .bind(u.version)
    .bind(&u.verifier)
    .fetch_optional(&mut *tx)
    .await?;
    if current.is_none() {
        tx.rollback().await?;
        return Ok(false);
    }
    let consumed = sqlx::query(
        "UPDATE dashboard_auth_tokens SET claim = $1 \
         WHERE hash = $2 AND user_id = $3 AND version = $4 AND purpose = 'email' AND claim IS NULL \
           AND expires_at > $5",
    )
    .bind(claim)
    .bind(token_hash)
    .bind(&u.id)
    .bind(u.version)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if consumed.rows_affected() != 1 {
        tx.rollback().await?;
        return Ok(false);
    }
    sqlx::query(
        "UPDATE dashboard_auth_users SET email = \
         (SELECT email FROM dashboard_auth_tokens WHERE hash = $1 AND claim = $2) WHERE id = $3",
    )
    .bind(token_hash)
    .bind(claim)
    .bind(&u.id)
    .execute(&mut *tx)
    .await?;
    insert_outbox(
        &mut *tx,
        &notice.id,
        &notice.payload,
        notice.next_at,
        notice.expires_at,
    )
    .await?;
    delete_user_sessions(&mut *tx, &u.id).await?;
    sqlx::query("DELETE FROM dashboard_auth_tokens WHERE user_id = $1 AND hash <> $2")
        .bind(&u.id)
        .bind(token_hash)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn due_outbox(pool: &PgPool, now: i64, max_attempts: i64) -> Result<Vec<OutboxRow>> {
    Ok(sqlx::query_as::<_, OutboxRow>(
        "SELECT id, payload, attempts FROM dashboard_auth_outbox \
         WHERE next_at <= $1 AND expires_at > $1 AND attempts < $2 LIMIT 20",
    )
    .bind(now)
    .bind(max_attempts)
    .fetch_all(pool)
    .await?)
}

/// Claim one delivery attempt. `false` if another worker got there first.
pub async fn claim_outbox(pool: &PgPool, id: &str, attempts: i64, next_at: i64) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE dashboard_auth_outbox SET attempts = attempts + 1, next_at = $1 \
         WHERE id = $2 AND attempts = $3",
    )
    .bind(next_at)
    .bind(id)
    .bind(attempts)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn delete_outbox(pool: &PgPool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM dashboard_auth_outbox WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn purge_expired(pool: &PgPool, now: i64) -> Result<()> {
    sqlx::query("DELETE FROM dashboard_auth_tokens WHERE expires_at < $1")
        .bind(now)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM dashboard_auth_sessions WHERE expires_at < $1 OR idle_until < $1")
        .bind(now)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM dashboard_auth_outbox WHERE expires_at < $1")
        .bind(now)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM dashboard_auth_limits WHERE expires_at < $1")
        .bind(now)
        .execute(pool)
        .await?;
    Ok(())
}
