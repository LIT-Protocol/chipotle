//! Postgres connection pool + migration helper.

use anyhow::{Context, Result};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

pub async fn connect(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await
        .context("connecting to Postgres")?;
    Ok(pool)
}

/// Run all pending migrations from `migrations/`.
///
/// GOTCHA: `sqlx::migrate!` embeds the migration SQL (and its checksums) into
/// the binary at COMPILE time, and does NOT cause cargo to recompile this crate
/// when only a file under `migrations/` changes. With a warm build cache (e.g.
/// Railway's), adding or editing a migration can ship a STALE binary that still
/// embeds the old SQL — which then panics at boot with "migration … was
/// previously applied but has been modified". When you touch `migrations/`, also
/// bump the marker below so this source file changes and the crate is forced to
/// recompile (re-embedding the current migrations).
///
/// migrations-embedded-through: 20260629000001
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("running migrations")?;
    Ok(())
}
