//! lit-chat — the consumer service: static frontend, auth/session routes,
//! chat CRUD, POST /api/chat/stream (SSE OpenRouter proxy), attestation.
//!
//! Logging gate (plans/tee-chat-app.md section 7.3): ships at `info`; no
//! message bodies, titles, emails, or raw user_refs are ever logged.

use lit_chat::crypto::keys::KeySource;
use lit_chat::mail::Mailer;
use lit_chat::openrouter::OpenRouterClient;
use lit_chat::rate_limit::RateLimiter;
use lit_chat::routes::RuntimeKeyCache;
use lit_chat::store::provider_keys;
use lit_chat::{attestation, config, db, headers, routes, Keyring};
use rocket::fs::FileServer;
use rocket::routes;

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}

fn apply_platform_env() {
    if std::env::var("ROCKET_PORT").is_err() {
        if let Ok(port) = std::env::var("PORT") {
            std::env::set_var("ROCKET_PORT", port);
        }
    }
    if std::env::var("ROCKET_ADDRESS").is_err() {
        std::env::set_var("ROCKET_ADDRESS", "0.0.0.0");
    }
}

/// Bootstrap-only import (section 4.4): `encrypted_env` keys move into the
/// encrypted provider_keys store on first boot; runtime custody lives there
/// so rotation never requires a redeploy.
async fn bootstrap_provider_keys(
    pool: &sqlx::PgPool,
    keyring: &Keyring,
    cfg: &config::Config,
) -> anyhow::Result<()> {
    if let Some(key) = &cfg.bootstrap_provisioning_key {
        if provider_keys::provisioning(pool, "openrouter")
            .await?
            .is_none()
        {
            let row = provider_keys::insert(
                pool,
                &keyring.provider_kek,
                "openrouter",
                "provisioning",
                key,
                "active",
                None,
                None,
                "bootstrap",
            )
            .await?;
            tracing::info!(key_id = %row.id, hint = %row.masked_hint, "bootstrapped provisioning key");
        }
    }
    if let Some(key) = &cfg.bootstrap_runtime_key {
        if provider_keys::active_runtime(pool, "openrouter")
            .await?
            .is_none()
        {
            let row = provider_keys::insert(
                pool,
                &keyring.provider_kek,
                "openrouter",
                "runtime",
                key,
                "active",
                None,
                None,
                "bootstrap",
            )
            .await?;
            tracing::info!(key_id = %row.id, hint = %row.masked_hint, "bootstrapped runtime key");
        }
    }
    Ok(())
}

#[rocket::launch]
async fn rocket() -> _ {
    init_tracing();
    apply_platform_env();

    let cfg = config::Config::from_env().expect("config");
    let key_source = KeySource::from_env().expect("key source");
    let keyring = Keyring::load(&key_source).await.expect("keyring (dstack)");
    let pool = db::connect(&cfg).await.expect("db connect");
    db::run_migrations(&pool).await.expect("db migrate");

    if let Err(e) = lit_chat::store::users::purge_expired(&pool).await {
        tracing::warn!("boot purge failed: {e}");
    }
    bootstrap_provider_keys(&pool, &keyring, &cfg)
        .await
        .expect("provider key bootstrap");

    let mailer = match (&cfg.resend_api_key, &cfg.mail_from) {
        (Some(key), Some(from)) => Some(Mailer::new(key.clone(), from.clone()).expect("mailer")),
        _ => None,
    };
    let or_client = OpenRouterClient::new(&cfg.openrouter_base_url).expect("openrouter client");

    rocket::build()
        .manage(pool)
        .manage(cfg)
        .manage(keyring)
        .manage(key_source)
        .manage(or_client)
        .manage(mailer)
        .manage(RateLimiter::new())
        .manage(RuntimeKeyCache::new())
        .attach(headers::SecurityHeaders)
        .mount("/", attestation::routes())
        .mount(
            "/",
            routes![
                routes::auth::anon_session,
                routes::auth::request_code,
                routes::auth::verify_code,
                routes::auth::logout,
                routes::chat::me,
                routes::chat::models,
                routes::chat::list_conversations,
                routes::chat::create_conversation,
                routes::chat::rename_conversation,
                routes::chat::delete_conversation,
                routes::chat::list_messages,
                routes::chat::stream,
                routes::chat::export,
                routes::chat::delete_account,
                routes::chat::status,
                routes::chat::health,
            ],
        )
        .mount("/", FileServer::from("static"))
}
