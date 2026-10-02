//! Encrypted mail outbox. Verification links and change notifications are
//! queued inside the transaction that makes them valid, encrypted at rest
//! under a key derived from the service secret, and delivered through Resend
//! with the row id as idempotency key. A periodic job retries failures and
//! purges expired rows; request handlers also kick an immediate attempt.

use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::db;
use super::{
    DashboardAuthConfig, MAX_OUTBOX_ATTEMPTS, NOTIFICATION_TTL_SECONDS, OUTBOX_RETRY_SECONDS,
    VERIFICATION_TTL_SECONDS, crypto, now,
};
use crate::mail::Mailer;

/// Encrypted outbox row ready to insert.
#[derive(Clone, Debug)]
pub struct Queued {
    pub id: String,
    pub payload: String,
    pub next_at: i64,
    pub expires_at: i64,
}

#[derive(Serialize, Deserialize)]
struct Message {
    to: String,
    subject: String,
    text: String,
}

#[derive(Serialize, Deserialize)]
struct StoredPayload {
    nonce: String,
    ciphertext: String,
}

fn encrypt(cfg: &DashboardAuthConfig, message: &Message, expires_at: i64) -> Result<Queued> {
    let id = crypto::random_hex(16);
    let nonce = crypto::random_hex(12);
    let plaintext = serde_json::to_vec(message).context("serialising mail")?;
    let ciphertext = crypto::encrypt_outbox(&cfg.secret, &id, &nonce, &plaintext)?;
    let payload = serde_json::to_string(&StoredPayload { nonce, ciphertext })
        .context("serialising outbox")?;
    Ok(Queued {
        id,
        payload,
        next_at: now(),
        expires_at,
    })
}

/// A change notification (password or email changed) to the given address.
pub fn notification(
    cfg: &DashboardAuthConfig,
    to: &str,
    subject: &str,
    text: &str,
) -> Result<Queued> {
    encrypt(
        cfg,
        &Message {
            to: to.to_string(),
            subject: subject.to_string(),
            text: text.to_string(),
        },
        now() + NOTIFICATION_TTL_SECONDS,
    )
}

/// Create a verification proof for `destination` and queue the email carrying
/// it, in one transaction. `purpose` is `signup` or `email` (address change,
/// bound to `user`). The link points back at the dashboard the request came
/// from.
pub async fn queue_verification(
    pool: &PgPool,
    cfg: &DashboardAuthConfig,
    dashboard_url: &str,
    destination: &str,
    purpose: &str,
    user: Option<&db::User>,
) -> Result<()> {
    let raw = crypto::random_hex(32);
    let hash = crypto::sha256_hex(&raw);
    let expires_at = now() + VERIFICATION_TTL_SECONDS;
    let link = format!("{dashboard_url}#verify={raw}&purpose={purpose}");
    let message = Message {
        to: destination.to_string(),
        subject: "Verify your Chipotle email".to_string(),
        text: format!(
            "Open this link to verify your email for Chipotle:\n\n{link}\n\nThis link expires in 30 minutes. It cannot reset a password. If you did not request it, ignore this email."
        ),
    };
    let queued = encrypt(cfg, &message, expires_at)?;
    let mut tx = pool.begin().await?;
    db::insert_token(
        &mut *tx,
        &hash,
        destination,
        user.map(|u| u.id.as_str()),
        user.map(|u| u.version),
        purpose,
        expires_at,
    )
    .await?;
    db::insert_outbox(
        &mut *tx,
        &queued.id,
        &queued.payload,
        queued.next_at,
        queued.expires_at,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// One delivery pass over due rows. Never panics; failures are logged with
/// fixed messages (plus the provider HTTP status) and retried later.
pub async fn deliver(pool: PgPool, cfg: DashboardAuthConfig, mailer: Mailer) {
    let rows = match db::due_outbox(&pool, now(), MAX_OUTBOX_ATTEMPTS).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("dashboard_auth_outbox_query_failed: {e}");
            return;
        }
    };
    for row in rows {
        match db::claim_outbox(&pool, &row.id, row.attempts, now() + OUTBOX_RETRY_SECONDS).await {
            Ok(true) => {}
            Ok(false) => continue,
            Err(e) => {
                tracing::warn!("dashboard_auth_outbox_claim_failed: {e}");
                continue;
            }
        }
        let sent = send_row(&cfg, &mailer, &row).await;
        match sent {
            Ok(()) => {
                if let Err(e) = db::delete_outbox(&pool, &row.id).await {
                    tracing::warn!("dashboard_auth_outbox_delete_failed: {e}");
                }
            }
            Err(status) => match status {
                Some(status) => tracing::error!(status, "auth_mail_delivery_failed"),
                None => tracing::error!("auth_mail_delivery_failed"),
            },
        }
    }
}

/// `Err(Some(status))` for a provider rejection, `Err(None)` for anything else.
async fn send_row(
    cfg: &DashboardAuthConfig,
    mailer: &Mailer,
    row: &db::OutboxRow,
) -> Result<(), Option<u16>> {
    let stored: StoredPayload = serde_json::from_str(&row.payload).map_err(|_| None)?;
    let plain = crypto::decrypt_outbox(&cfg.secret, &row.id, &stored.nonce, &stored.ciphertext)
        .map_err(|_| None)?;
    let message: Message = serde_json::from_slice(&plain).map_err(|_| None)?;
    mailer
        .send_queued(&message.to, &message.subject, &message.text, &row.id)
        .await
        .map_err(|e| e.status())
}

/// Periodic delivery + cleanup loop. The first tick runs immediately so mail
/// queued before a restart goes out at boot.
pub fn spawn(pool: PgPool, cfg: DashboardAuthConfig, mailer: Mailer) {
    let interval = Duration::from_secs(cfg.outbox_interval_secs);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            deliver(pool.clone(), cfg.clone(), mailer.clone()).await;
            if let Err(e) = db::purge_expired(&pool, now()).await {
                tracing::warn!("dashboard_auth_purge_failed: {e}");
            }
        }
    });
}
