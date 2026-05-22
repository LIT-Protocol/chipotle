//! Periodic scheduler for `kind = schedule` triggers.
//!
//! The scheduler deliberately uses Postgres as the durable source of truth. Each
//! scan acquires a transaction-scoped advisory lock, computes at most one due
//! cron tick per trigger from `schedule_watermarks`, enqueues a `trigger_runs`
//! row, and advances the watermark in the same transaction.

use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use cron::Schedule;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::config::Config;

const SCAN_INTERVAL: Duration = Duration::from_secs(30);
const SCHEDULER_LOCK_KEY: i64 = 0x6c69_745f_7363_6865; // "lit_sche" prefix, arbitrary app lock.

#[derive(Debug)]
struct ScheduleTrigger {
    id: Uuid,
    config: Value,
    created_at: OffsetDateTime,
    max_queued_runs: Option<i32>,
    last_enqueued_at: Option<OffsetDateTime>,
}

pub async fn run(pool: PgPool, config: Config) {
    loop {
        if let Err(e) = scan_once(&pool, &config).await {
            tracing::warn!("schedule scan failed: {e}");
        }
        tokio::time::sleep(SCAN_INTERVAL).await;
    }
}

async fn scan_once(pool: &PgPool, config: &Config) -> Result<()> {
    let mut tx = pool.begin().await?;

    let locked = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_xact_lock($1)")
        .bind(SCHEDULER_LOCK_KEY)
        .fetch_one(&mut *tx)
        .await?;
    if !locked {
        return Ok(());
    }

    let rows = sqlx::query(
        "SELECT t.id, t.config, t.created_at, t.max_queued_runs, w.last_enqueued_at
         FROM triggers t
         LEFT JOIN schedule_watermarks w ON w.trigger_id = t.id
         WHERE t.kind = 'schedule' AND t.enabled = true
         ORDER BY t.created_at ASC
         FOR UPDATE OF t",
    )
    .fetch_all(&mut *tx)
    .await?;

    let now = OffsetDateTime::now_utc();
    for row in rows {
        let trigger = ScheduleTrigger {
            id: row.get("id"),
            config: row.get("config"),
            created_at: row.get("created_at"),
            max_queued_runs: row.get("max_queued_runs"),
            last_enqueued_at: row.get("last_enqueued_at"),
        };

        if let Err(e) = enqueue_if_due(&mut tx, trigger, now, config).await {
            tracing::warn!("schedule trigger scan skipped: {e}");
        }
    }

    tx.commit().await?;
    Ok(())
}

async fn enqueue_if_due(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    trigger: ScheduleTrigger,
    now: OffsetDateTime,
    config: &Config,
) -> Result<()> {
    let cron = cron_expr_from_config(&trigger.config)
        .with_context(|| format!("trigger {} missing schedule cron", trigger.id))?;
    let scheduled_at =
        match next_due_tick(&cron, trigger.last_enqueued_at, trigger.created_at, now)? {
            Some(tick) => tick,
            None => return Ok(()),
        };

    let input = build_schedule_input(&cron, scheduled_at)?;

    if queue_depth(tx, trigger.id).await? >= max_queued_runs(&trigger, config) {
        tracing::warn!(trigger_id = %trigger.id, scheduled_at = %scheduled_at, "schedule trigger queue is full; leaving watermark unchanged");
        return Ok(());
    }

    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO trigger_runs (id, trigger_id, status, input, attempt)
         VALUES ($1, $2, 'queued', $3, 1)",
    )
    .bind(run_id)
    .bind(trigger.id)
    .bind(input)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "INSERT INTO schedule_watermarks (trigger_id, last_enqueued_at, updated_at)
         VALUES ($1, $2, now())
         ON CONFLICT (trigger_id) DO UPDATE
         SET last_enqueued_at = EXCLUDED.last_enqueued_at,
             updated_at = now()",
    )
    .bind(trigger.id)
    .bind(scheduled_at)
    .execute(&mut **tx)
    .await?;

    tracing::info!(trigger_id = %trigger.id, run_id = %run_id, scheduled_at = %scheduled_at, "queued scheduled trigger run");
    Ok(())
}

async fn queue_depth(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    trigger_id: Uuid,
) -> Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM trigger_runs
         WHERE trigger_id = $1 AND status IN ('queued','running','retrying')",
    )
    .bind(trigger_id)
    .fetch_one(&mut **tx)
    .await
    .context("checking schedule trigger queue depth")
}

fn max_queued_runs(trigger: &ScheduleTrigger, config: &Config) -> i64 {
    trigger
        .max_queued_runs
        .unwrap_or(config.webhook_default_max_queued_runs as i32)
        .max(0) as i64
}

pub fn cron_expr_from_config(config: &Value) -> Option<String> {
    config
        .get("cron")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

pub fn normalize_cron(expr: &str) -> Result<String> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    let normalized = match fields.len() {
        5 => format!("0 {expr}"),
        6 => expr.to_string(),
        _ => anyhow::bail!("cron expression must have 5 fields or 6 fields with seconds"),
    };
    let schedule = Schedule::from_str(&normalized).context("invalid cron expression")?;
    validate_min_interval(&schedule)?;
    Ok(normalized)
}

fn validate_min_interval(schedule: &Schedule) -> Result<()> {
    let start = DateTime::from_timestamp(1_800_000_000, 0).context("fixed timestamp valid")?;
    let mut upcoming = schedule.after(&start);
    let mut prev = upcoming
        .next()
        .context("cron expression has no future occurrences")?;
    for _ in 0..10 {
        let next = upcoming
            .next()
            .context("cron expression has too few future occurrences")?;
        if next.signed_duration_since(prev).num_seconds() < SCAN_INTERVAL.as_secs() as i64 {
            anyhow::bail!(
                "cron interval must be at least {} seconds",
                SCAN_INTERVAL.as_secs()
            );
        }
        prev = next;
    }
    Ok(())
}

pub fn validate_cron(expr: &str) -> Result<()> {
    normalize_cron(expr).map(|_| ())
}

pub fn next_due_tick(
    expr: &str,
    last_enqueued_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    now: OffsetDateTime,
) -> Result<Option<OffsetDateTime>> {
    let normalized = normalize_cron(expr)?;
    let schedule = Schedule::from_str(&normalized)?;
    let baseline = last_enqueued_at.unwrap_or(created_at);
    let Some(next) = schedule.after(&to_chrono(baseline)?).next() else {
        return Ok(None);
    };
    let next = from_chrono(next)?;
    if next <= now {
        Ok(Some(next))
    } else {
        Ok(None)
    }
}

pub fn build_schedule_input(expr: &str, scheduled_at: OffsetDateTime) -> Result<Value> {
    Ok(json!({
        "source": "schedule",
        "scheduled_at": scheduled_at.format(&time::format_description::well_known::Rfc3339)?,
        "cron": expr,
    }))
}

fn to_chrono(t: OffsetDateTime) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp(t.unix_timestamp(), t.nanosecond()).context("timestamp out of range")
}

fn from_chrono(t: DateTime<Utc>) -> Result<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp(t.timestamp())
        .context("timestamp out of range")?
        .replace_nanosecond(t.timestamp_subsec_nanos())
        .context("nanosecond out of range")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::macros::datetime;

    #[test]
    fn normalizes_five_field_cron_by_prefixing_seconds() {
        assert_eq!(normalize_cron("*/5 * * * *").unwrap(), "0 */5 * * * *");
    }

    #[test]
    fn accepts_six_field_cron_with_seconds() {
        assert_eq!(normalize_cron("30 */5 * * * *").unwrap(), "30 */5 * * * *");
    }

    #[test]
    fn rejects_invalid_cron() {
        assert!(normalize_cron("not a cron").is_err());
        assert!(normalize_cron("* * * *").is_err());
    }

    #[test]
    fn rejects_sub_scan_interval_cron() {
        assert!(normalize_cron("*/10 * * * * *").is_err());
    }

    #[test]
    fn computes_at_most_one_due_tick_after_watermark() {
        let due = next_due_tick(
            "*/5 * * * *",
            Some(datetime!(2026-05-22 13:00 UTC)),
            datetime!(2026-05-22 12:00 UTC),
            datetime!(2026-05-22 13:12 UTC),
        )
        .unwrap();
        assert_eq!(due, Some(datetime!(2026-05-22 13:05 UTC)));
    }

    #[test]
    fn returns_none_when_next_tick_is_in_future() {
        let due = next_due_tick(
            "*/5 * * * *",
            Some(datetime!(2026-05-22 13:00 UTC)),
            datetime!(2026-05-22 12:00 UTC),
            datetime!(2026-05-22 13:04 UTC),
        )
        .unwrap();
        assert_eq!(due, None);
    }

    #[test]
    fn builds_schedule_input_payload() {
        let input = build_schedule_input("*/5 * * * *", datetime!(2026-05-22 13:05 UTC)).unwrap();
        assert_eq!(
            input,
            json!({
                "source": "schedule",
                "scheduled_at": "2026-05-22T13:05:00Z",
                "cron": "*/5 * * * *"
            })
        );
    }
}
