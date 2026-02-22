//! Local SQLite lookup database: `lookup.db` in the application root.
//! Single table `wallets` with keyHash → (pubkey, wallet) lookups.

use anyhow::Result;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::str::FromStr;

const DB_FILENAME: &str = "lookup.db";

/// Returns the path to the lookup database file (application root / `lookup.db`).
/// Uses the current working directory as the application root.
pub fn db_path() -> Result<PathBuf> {
    let root = std::env::current_dir()?;
    Ok(root.join(DB_FILENAME))
}

async fn open_pool(db_path: &Path) -> Result<SqlitePool> {
    let path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid DB path"))?;
    let opts = SqliteConnectOptions::from_str(path_str)?.create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;
    Ok(pool)
}

/// Creates the database file and `wallets` table if they do not exist.
/// Safe to call at startup; no-op if the table already exists.
pub async fn ensure_table() -> Result<()> {
    let pool = open_pool(&db_path()?).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS wallets (keyHash TEXT NOT NULL, pubkey TEXT NOT NULL, wallet TEXT NOT NULL)",
    )
    .execute(&pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_wallets_keyHash ON wallets (keyHash)")
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}

/// Inserts a row into the `wallets` table.
pub async fn add_wallet(key_hash: &str, pubkey: &str, wallet: &str) -> Result<()> {
    let pool = open_pool(&db_path()?).await?;
    sqlx::query("INSERT INTO wallets (keyHash, pubkey, wallet) VALUES (?1, ?2, ?3)")
        .bind(key_hash)
        .bind(pubkey)
        .bind(wallet)
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}

/// Lookup result: `pubkey` and `wallet` for a given `keyHash`.
#[derive(Debug, Clone)]
pub struct LookupResult {
    pub pubkey: String,
    pub wallet: String,
}

/// Looks up `pubkey` and `wallet` by `keyHash`. Returns `None` if not found.
pub async fn lookup_by_key_hash(key_hash: &str) -> Result<Option<LookupResult>> {
    let pool = open_pool(&db_path()?).await?;
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT pubkey, wallet FROM wallets WHERE keyHash = ?1 LIMIT 1",
    )
    .bind(key_hash)
    .fetch_optional(&pool)
    .await?;
    pool.close().await;
    Ok(row.map(|(pubkey, wallet)| LookupResult { pubkey, wallet }))
}
