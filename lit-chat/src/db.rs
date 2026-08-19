//! Postgres pool + migrations.
//!
//! Unlike the lit-payments/lit-triggers `db.rs` this repo otherwise copies,
//! lit-chat compiles a TLS backend (rustls) and supports
//! `sslmode=verify-full` with a pinned CA: the chat DB lives off-TEE on
//! Railway and the connection leaves the enclave, so opportunistic-TLS
//! ("prefer", sqlx's default) is not acceptable here.

use crate::config::Config;
use anyhow::{Context, Result};
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions, PgSslMode};
use std::str::FromStr;
use std::time::Duration;

pub async fn connect(cfg: &Config) -> Result<PgPool> {
    let mut opts = PgConnectOptions::from_str(&cfg.database_url).context("parsing DATABASE_URL")?;
    if let Some(pem) = &cfg.db_ca_cert_pem {
        opts = opts
            .ssl_mode(PgSslMode::VerifyFull)
            .ssl_root_cert_from_pem(pem.clone());
    }
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(opts)
        .await
        .context("connecting to Postgres")?;
    Ok(pool)
}

/// Run all pending migrations from `migrations/`. Only the consumer binary
/// migrates; the admin role has no DDL grants (db/roles.sql).
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("running migrations")?;
    Ok(())
}
