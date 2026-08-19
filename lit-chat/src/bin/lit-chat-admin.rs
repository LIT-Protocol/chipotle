//! lit-chat-admin — the admin console (plans/tee-chat-app.md section 4.4).
//!
//! Separate binary, separate port, separate ingress exposure, separate DB
//! role (db/roles.sql: no grants on chat tables). Zero user-data routes.
//! Does NOT run migrations — the consumer binary owns the schema.

use lit_chat::admin::{self, WebauthnState};
use lit_chat::crypto::keys::KeySource;
use lit_chat::mail::Mailer;
use lit_chat::openrouter::OpenRouterClient;
use lit_chat::rate_limit::RateLimiter;
use lit_chat::{attestation, config, db, headers, Keyring};
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
        } else {
            std::env::set_var("ROCKET_PORT", "8100");
        }
    }
    if std::env::var("ROCKET_ADDRESS").is_err() {
        std::env::set_var("ROCKET_ADDRESS", "0.0.0.0");
    }
}

#[rocket::launch]
async fn rocket() -> _ {
    init_tracing();
    apply_platform_env();

    let cfg = config::Config::from_env().expect("config");
    let key_source = KeySource::from_env().expect("key source");
    let keyring = Keyring::load(&key_source).await.expect("keyring (dstack)");
    let pool = db::connect(&cfg).await.expect("db connect");

    // MAC'd bootstrap roster from encrypted_env.
    admin::bootstrap_admins(&pool, &keyring, &cfg.bootstrap_admins)
        .await
        .expect("admin roster bootstrap");

    let rp_id = cfg
        .admin_rp_id
        .clone()
        .expect("LIT_CHAT_ADMIN_RP_ID is required for the admin console");
    let origin = cfg
        .admin_origin
        .clone()
        .expect("LIT_CHAT_ADMIN_ORIGIN is required for the admin console");
    let webauthn = WebauthnState::new(&rp_id, &origin).expect("webauthn");

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
        .manage(webauthn)
        .manage(RateLimiter::new())
        .attach(headers::SecurityHeaders)
        .mount("/", attestation::routes())
        .mount(
            "/",
            routes![
                admin::routes::auth_request,
                admin::routes::auth_verify,
                admin::routes::logout,
                admin::routes::me,
                admin::routes::webauthn_register_start,
                admin::routes::webauthn_register_finish,
                admin::routes::webauthn_auth_start,
                admin::routes::webauthn_auth_finish,
                admin::routes::list_keys,
                admin::routes::import_key,
                admin::routes::mint_key,
                admin::routes::promote_key,
                admin::routes::retire_key,
                admin::routes::disable_key,
                admin::routes::probe_key,
                admin::routes::status,
                admin::routes::set_breaker,
                admin::routes::set_caps,
                admin::routes::list_models,
                admin::routes::toggle_model,
                admin::routes::list_admins,
                admin::routes::grant_admin,
                admin::routes::revoke_admin,
                admin::routes::audit_log,
                admin::routes::health,
            ],
        )
        .mount("/", FileServer::from("admin-static"))
}
