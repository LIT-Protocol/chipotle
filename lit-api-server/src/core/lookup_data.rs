//! Local SQLite lookup database: `lookup.db` in the application root.
//! Tables: `wallets` (keyHash → pubkey, wallet_address, secret), `api_keys` (keyHash → apiKey).

use anyhow::Result;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
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
    let need_tables = !db_path.exists();

    let path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid DB path"))?;

    let opts = SqliteConnectOptions::from_str(path_str)?.create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;
    if need_tables {
        ensure_tables(&pool).await?;
    }
    Ok(pool)
}

/// Creates the database file and tables (`wallets`, `api_keys`) if they do not exist.
/// Safe to call at startup; no-op if the tables already exist.
pub async fn ensure_tables(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS wallets (keyHash TEXT NOT NULL, pubkey TEXT NOT NULL, wallet_address TEXT NOT NULL, secret TEXT NOT NULL)",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_wallets_keyHash ON wallets (keyHash)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS api_keys (keyHash TEXT NOT NULL, apiKey TEXT NOT NULL)",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_api_keys_keyHash ON api_keys (keyHash)")
        .execute(pool)
        .await?;
    Ok(())
}

/// Inserts a row into the `wallets` table.
pub async fn add_wallet(
    key_hash: &str,
    pubkey: &str,
    wallet_address: &str,
    secret: &str,
) -> Result<()> {
    let pool = open_pool(&db_path()?).await?;
    sqlx::query(
        "INSERT INTO wallets (keyHash, pubkey, wallet_address, secret) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(key_hash)
    .bind(pubkey)
    .bind(wallet_address)
    .bind(secret)
    .execute(&pool)
    .await?;
    pool.close().await;
    Ok(())
}

/// Lookup result: `pubkey`, `wallet_address`, and `secret` for a given `keyHash`.
#[derive(Debug, Clone)]
pub struct LookupResult {
    pub pubkey: String,
    pub wallet_address: String,
    pub secret: String,
}

/// Looks up `pubkey`, `wallet_address`, and `secret` by `keyHash`. Returns `None` if not found.
pub async fn lookup_by_key_hash(key_hash: &str) -> Result<Option<LookupResult>> {
    let pool = open_pool(&db_path()?).await?;
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT pubkey, wallet_address, secret FROM wallets WHERE keyHash = ?1 LIMIT 1",
    )
    .bind(key_hash)
    .fetch_optional(&pool)
    .await?;
    pool.close().await;
    Ok(row.map(|(pubkey, wallet_address, secret)| LookupResult {
        pubkey,
        wallet_address,
        secret,
    }))
}

/// Looks up wallets for multiple `keyHash` values. Returns a vec of `(key_hash, LookupResult)` for each match.
/// Keys not found are omitted from the result. Empty input returns an empty vec.
pub async fn lookup_wallets_by_key_hashes(
    key_hashes: &[impl AsRef<str>],
) -> Result<Vec<(String, LookupResult)>> {
    if key_hashes.is_empty() {
        return Ok(Vec::new());
    }

    let pool = open_pool(&db_path()?).await?;
    let placeholders = key_hashes
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!(
        "SELECT keyHash, pubkey, wallet_address, secret FROM wallets WHERE keyHash IN ({})",
        placeholders
    );
    let mut q = sqlx::query_as::<_, (String, String, String, String)>(&query);
    for k in key_hashes {
        q = q.bind(k.as_ref());
    }
    let rows = q.fetch_all(&pool).await?;
    pool.close().await;
    Ok(rows
        .into_iter()
        .map(|(key_hash, pubkey, wallet_address, secret)| {
            (
                key_hash,
                LookupResult {
                    pubkey,
                    wallet_address,
                    secret,
                },
            )
        })
        .collect())
}

// ---------- api_keys table ----------

/// Inserts a row into the `api_keys` table.
pub async fn add_api_key(key_hash: &str, api_key: &str) -> Result<()> {
    let pool = open_pool(&db_path()?).await?;
    sqlx::query("INSERT INTO api_keys (keyHash, apiKey) VALUES (?1, ?2)")
        .bind(key_hash)
        .bind(api_key)
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}

/// Looks up `apiKey` by `keyHash`. Returns `None` if not found.
pub async fn get_api_key_by_key_hash(key_hash: &str) -> Result<Option<String>> {
    let pool = open_pool(&db_path()?).await?;
    let row: Option<(String,)> =
        sqlx::query_as("SELECT apiKey FROM api_keys WHERE keyHash = ?1 LIMIT 1")
            .bind(key_hash)
            .fetch_optional(&pool)
            .await?;
    pool.close().await;
    Ok(row.map(|(api_key,)| api_key))
}

/// Looks up api keys for multiple `keyHash` values. Returns a vec of `(key_hash, api_key)` for each match.
/// Keys not found are omitted from the result. Empty input returns an empty vec.
pub async fn get_api_keys_by_key_hashes(
    key_hashes: &[impl AsRef<str>],
) -> Result<Vec<(String, String)>> {
    if key_hashes.is_empty() {
        return Ok(Vec::new());
    }

    let pool = open_pool(&db_path()?).await?;
    let placeholders = key_hashes
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!(
        "SELECT keyHash, apiKey FROM api_keys WHERE keyHash IN ({})",
        placeholders
    );
    let mut q = sqlx::query_as::<_, (String, String)>(&query);
    for k in key_hashes {
        q = q.bind(k.as_ref());
    }
    let rows = q.fetch_all(&pool).await?;
    pool.close().await;
    Ok(rows)
}

/// Deletes the api_keys row with the given `keyHash`. No-op if not found.
pub async fn delete_api_key_by_key_hash(key_hash: &str) -> Result<()> {
    let pool = open_pool(&db_path()?).await?;
    sqlx::query("DELETE FROM api_keys WHERE keyHash = ?1")
        .bind(key_hash)
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}
