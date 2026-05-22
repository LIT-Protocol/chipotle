//! Async run dispatcher: claims queued trigger runs and executes Lit Actions.

use std::time::Duration;

use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::chipotle::{ChipotleClient, ChipotleError};
use crate::config::Config;
use crate::crypto;

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const MAX_ATTEMPTS: i32 = 3;
const RUNNING_STALE_AFTER: &str = "5 minutes";

#[derive(Debug)]
struct ClaimedRun {
    id: Uuid,
    trigger_id: Uuid,
    input: Option<Value>,
    attempt: i32,
    action_code: String,
    default_params: Value,
    usage_api_key_ciphertext: Vec<u8>,
    usage_api_key_nonce: Vec<u8>,
}

pub async fn run(pool: PgPool, config: Config) {
    let chipotle = ChipotleClient::new(config.chipotle_api_base_url.clone());
    loop {
        match claim_one(&pool).await {
            Ok(Some(run)) => execute_claimed_run(&pool, &config, &chipotle, run).await,
            Ok(None) => tokio::time::sleep(POLL_INTERVAL).await,
            Err(e) => {
                tracing::warn!("run dispatcher claim failed: {e}");
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        }
    }
}

async fn claim_one(pool: &PgPool) -> anyhow::Result<Option<ClaimedRun>> {
    recover_stale_runs(pool).await?;

    let row = sqlx::query(
        "UPDATE trigger_runs r
         SET status = 'running',
             claimed_at = now(),
             next_attempt_at = NULL,
             attempt = CASE WHEN r.status = 'retrying' THEN r.attempt + 1 ELSE r.attempt END
         FROM triggers t,
         (
           SELECT r2.id
           FROM trigger_runs r2
           JOIN triggers t2 ON t2.id = r2.trigger_id
           WHERE (r2.status = 'queued'
                  OR (r2.status = 'retrying'
                      AND (r2.next_attempt_at IS NULL OR r2.next_attempt_at <= now())))
             AND t2.enabled = true
             AND NOT EXISTS (
               SELECT 1 FROM trigger_runs active
               WHERE active.trigger_id = r2.trigger_id
                 AND active.status = 'running'
                 AND active.claimed_at > now() - ($1::text)::interval
             )
           ORDER BY COALESCE(r2.next_attempt_at, r2.started_at) ASC
           FOR UPDATE OF r2 SKIP LOCKED
           LIMIT 1
         ) claim
         WHERE r.id = claim.id AND t.id = r.trigger_id
         RETURNING r.id, r.trigger_id, r.input, r.attempt,
                   t.action_code, t.default_params,
                   t.usage_api_key_ciphertext, t.usage_api_key_nonce",
    )
    .bind(RUNNING_STALE_AFTER)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| ClaimedRun {
        id: row.get("id"),
        trigger_id: row.get("trigger_id"),
        input: row.get("input"),
        attempt: row.get("attempt"),
        action_code: row.get("action_code"),
        default_params: row.get("default_params"),
        usage_api_key_ciphertext: row.get("usage_api_key_ciphertext"),
        usage_api_key_nonce: row.get("usage_api_key_nonce"),
    }))
}

async fn recover_stale_runs(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE trigger_runs
         SET status = 'queued', claimed_at = NULL, error = COALESCE(error, 'recovered stale running run')
         WHERE status = 'running'
           AND claimed_at <= now() - ($1::text)::interval",
    )
    .bind(RUNNING_STALE_AFTER)
    .execute(pool)
    .await?;
    Ok(())
}

async fn execute_claimed_run(
    pool: &PgPool,
    config: &Config,
    chipotle: &ChipotleClient,
    run: ClaimedRun,
) {
    let usage_key = match crypto::decrypt_usage_key(
        &config.usage_key_encryption_key,
        &run.usage_api_key_nonce,
        &run.usage_api_key_ciphertext,
    ) {
        Ok(key) => key,
        Err(e) => {
            tracing::warn!(run_id = %run.id, trigger_id = %run.trigger_id, "usage key decrypt failed: {e}");
            mark_failed(pool, run.id, "usage key decrypt failed", None).await;
            return;
        }
    };

    let params = merge_params(
        run.default_params.clone(),
        run.input.clone().unwrap_or(Value::Null),
    );
    match chipotle
        .execute_lit_action(&usage_key, run.action_code.clone(), params)
        .await
    {
        Ok(response) => mark_success(pool, run.id, response).await,
        Err(err) => handle_chipotle_error(pool, run, err).await,
    }
}

fn merge_params(default_params: Value, input: Value) -> Value {
    let mut params = match default_params {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };

    match input {
        Value::Object(input_obj) => {
            for (key, value) in input_obj {
                params.insert(key, value);
            }
        }
        other => {
            params.insert("event".to_string(), other);
        }
    }

    Value::Object(params)
}

async fn handle_chipotle_error(pool: &PgPool, run: ClaimedRun, err: ChipotleError) {
    if err.transient && run.attempt < MAX_ATTEMPTS {
        let delay = retry_delay(run.attempt);
        if let Err(e) = sqlx::query(
            "UPDATE trigger_runs
             SET status = 'retrying', error = $2, claimed_at = NULL,
                 next_attempt_at = now() + ($3::text)::interval
             WHERE id = $1",
        )
        .bind(run.id)
        .bind(err.message)
        .bind(format!("{} milliseconds", delay.as_millis()))
        .execute(pool)
        .await
        {
            tracing::warn!(run_id = %run.id, "failed to mark run retrying: {e}");
        }
    } else {
        mark_failed(pool, run.id, &err.message, err.response).await;
    }
}

fn retry_delay(attempt: i32) -> Duration {
    match attempt {
        1 => Duration::from_secs(1),
        2 => Duration::from_secs(5),
        _ => Duration::from_secs(30),
    }
}

async fn mark_success(pool: &PgPool, run_id: Uuid, response: Value) {
    if let Err(e) = sqlx::query(
        "UPDATE trigger_runs
         SET status = 'success', finished_at = now(), response = $2, error = NULL,
             claimed_at = NULL, next_attempt_at = NULL
         WHERE id = $1",
    )
    .bind(run_id)
    .bind(response)
    .execute(pool)
    .await
    {
        tracing::warn!(run_id = %run_id, "failed to mark run success: {e}");
    }
}

async fn mark_failed(pool: &PgPool, run_id: Uuid, error: &str, response: Option<Value>) {
    if let Err(e) = sqlx::query(
        "UPDATE trigger_runs
         SET status = 'failed', finished_at = now(), error = $2, response = $3,
             claimed_at = NULL, next_attempt_at = NULL
         WHERE id = $1",
    )
    .bind(run_id)
    .bind(error)
    .bind(response)
    .execute(pool)
    .await
    {
        tracing::warn!(run_id = %run_id, "failed to mark run failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_params_overlays_webhook_input_on_defaults() {
        let merged = merge_params(
            json!({ "existing": true, "event": "default" }),
            json!({ "source": "webhook", "event": { "hello": "world" } }),
        );
        assert_eq!(
            merged,
            json!({
                "existing": true,
                "source": "webhook",
                "event": { "hello": "world" }
            })
        );
    }

    #[test]
    fn retry_delays_are_exponentialish() {
        assert_eq!(retry_delay(1), Duration::from_secs(1));
        assert_eq!(retry_delay(2), Duration::from_secs(5));
        assert_eq!(retry_delay(3), Duration::from_secs(30));
    }
}
