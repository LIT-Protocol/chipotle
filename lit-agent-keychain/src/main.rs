use lit_agent_keychain::{chipotle::Chipotle, config::Config, db, server};
#[rocket::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    if std::env::var("ROCKET_PORT").is_err() {
        if let Ok(port) = std::env::var("PORT") {
            std::env::set_var("ROCKET_PORT", port);
        }
    }
    if std::env::var("ROCKET_ADDRESS").is_err() {
        std::env::set_var("ROCKET_ADDRESS", "0.0.0.0");
    }
    let cfg = Config::from_env()?;
    let pool = db::connect(&cfg.database_url).await?;
    db::run_migrations(&pool).await?;
    // Bound transient authentication and rate-limit state on long-lived servers.
    // Budget windows are at most one day; retaining two days never resets a live window.
    let cleanup_pool = pool.clone();
    let cleanup = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(600));
        loop {
            interval.tick().await;
            for query in [
                "DELETE FROM kc_sessions WHERE expires_at<now()",
                "DELETE FROM kc_challenges WHERE expires_at<now()",
                "DELETE FROM kc_budgets WHERE expires_at<now()",
            ] {
                if sqlx::query(query).execute(&cleanup_pool).await.is_err() {
                    tracing::warn!("transient state cleanup failed");
                }
            }
        }
    });
    let lit = Chipotle::new(cfg.lit_api_url.clone(), cfg.lit_execution_key.clone())?;
    let result = server::build(cfg, pool, lit).launch().await;
    cleanup.abort();
    result?;
    Ok(())
}
