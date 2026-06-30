//! Postgres connection pool + migration helper.

use anyhow::{Context, Result};
use sqlx::migrate::Migrator;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::path::Path;
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

/// Run all pending migrations, loaded from the `./migrations` directory at
/// RUNTIME.
///
/// We deliberately do NOT use `sqlx::migrate!("./migrations")`. That macro
/// embeds the migration SQL (and its checksums) into the binary at COMPILE
/// time, and cargo does not reliably recompile this crate when only a file
/// under `migrations/` changes (proc-macros can't emit `rerun-if-changed` for
/// their inputs). The result: an incremental build can ship a binary whose
/// embedded migrations are STALE relative to the source files, which then
/// panics at boot with "migration … was previously applied but has been
/// modified" even though the committed files and the database agree.
///
/// Loading at runtime via [`Migrator::new`] reads the actual deployed
/// `./migrations` directory (the Docker image copies it to `/app/migrations`,
/// and the working directory is `/app`), so the migration set always matches
/// what's on disk — no stale-embed failure mode. We log each loaded migration's
/// version + checksum first so any future mismatch is diagnosable from the logs
/// instead of guessing.
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    let migrator = Migrator::new(Path::new("./migrations"))
        .await
        .context("loading migrations from ./migrations")?;
    for m in migrator.iter() {
        tracing::info!(
            version = m.version,
            checksum = %hex::encode(m.checksum.as_ref()),
            "migration loaded"
        );
    }
    migrator.run(pool).await.context("running migrations")?;
    Ok(())
}
