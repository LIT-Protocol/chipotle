//! Operator-only custom plan configuration. No owner/secret permissions are modified.
use anyhow::{bail, Context};
use lit_agent_keychain::{db, models::valid_hex, subscriptions::MAX_CUSTOM_LIMIT};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 || !valid_hex(&args[0], 32) {
        bail!("Usage: keychain-plan <vault-id> <secret-limit> <access-until-RFC3339>; set DATABASE_URL");
    }
    let limit: i64 = args[1].parse()?;
    if !(1..=MAX_CUSTOM_LIMIT).contains(&limit) {
        bail!("limit must be 1..{MAX_CUSTOM_LIMIT}");
    }
    let until = OffsetDateTime::parse(&args[2], &Rfc3339)?;
    let pool = db::connect(&std::env::var("DATABASE_URL").context("missing DATABASE_URL")?).await?;
    let mut tx = pool.begin().await?;
    // Taking the same lock as storage writes makes upgrades/downgrades atomic.
    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM kc_vaults WHERE id=$1 FOR UPDATE")
            .bind(&args[0])
            .fetch_optional(&mut *tx)
            .await?;
    if exists.is_none() {
        bail!("vault not found");
    }
    sqlx::query("INSERT INTO kc_subscriptions(vault_id,custom_secret_limit,custom_until) VALUES($1,$2,$3) ON CONFLICT(vault_id) DO UPDATE SET custom_secret_limit=$2,custom_until=$3,reconcile_after=now()")
        .bind(&args[0]).bind(limit).bind(until).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO kc_audit(vault_id,event) VALUES($1,'custom_plan_updated')")
        .bind(&args[0])
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    println!("Custom plan updated. Execution scope will reconcile automatically.");
    Ok(())
}
