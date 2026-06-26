use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use lit_billing_core::StripeClient;
use lit_billing_core::billing_auth::AuthResolver;
use lit_payments::auth::routes as auth_routes;
use lit_payments::auth_resolver::LocalAuthResolver;
use lit_payments::auto_topup::webhook::handler as webhook_handler;
use lit_payments::auto_topup::webhook::mutex::PerCustomerMutex;
use lit_payments::billing as billing_routes;
use lit_payments::portal::routes as portal_routes;
use lit_payments::rate;
use lit_payments::{auth, chain, config, db, mail};
use rocket::fs::{FileServer, NamedFile};
use rocket::http::Status;
use rocket::response::Redirect;
use rocket::{get, routes};

#[rocket::launch]
async fn rocket() -> _ {
    init_tracing();
    apply_platform_env();
    let cfg = config::Config::from_env().expect("config");
    let pool = db::connect(&cfg.database_url).await.expect("db connect");
    db::run_migrations(&pool).await.expect("db migrate");

    // Best-effort cleanup of expired sessions on boot.
    if let Err(e) = auth::session::purge_expired(&pool).await {
        tracing::warn!("session purge on boot failed: {e}");
    }

    let mailer =
        mail::Mailer::new(cfg.resend_api_key.clone(), cfg.mail_from.clone()).expect("mailer");
    let rate_limit = auth::rate_limit::RateLimiter::new();
    let stripe = StripeClient::new(cfg.stripe_secret_key.clone()).expect("stripe client");
    // In-process auth resolver — wallet-sig verification + on-chain
    // billing-wallet lookup, both running locally. Post-#448-glitch-followup
    // these used to be HTTP hops to lit-api-server; the shared
    // `lit-billing-core` crate now exposes both primitives so the same
    // resolver shape runs on both services.
    let on_chain_resolver = config::build_on_chain_resolver(&cfg).expect("OnChainBillingResolver");
    let auth_resolver: Arc<dyn AuthResolver> = Arc::new(LocalAuthResolver::new(
        on_chain_resolver,
        cfg.lit_accounts_chain_id,
    ));
    rate::spawn_rate_poller(pool.clone());
    let per_customer_mutex = PerCustomerMutex::new();
    lit_payments::auto_topup::reconciler::spawn(cfg.clone(), stripe.clone(), pool.clone());
    // Enterprise committed-use billing: monthly draft invoice + buffer regrant.
    // See plans/enterprise-committed-billing.md.
    lit_payments::enterprise::spawn(
        cfg.clone(),
        stripe.clone(),
        pool.clone(),
        mailer.clone(),
    );

    // Codex P1 (Phase 8): exact-match CORS allowlist driven by
    // `cors_allowed_origins`. The prior config used
    // `AllowedOrigins::all()` paired with `allow_credentials: true`,
    // which is CSRF-on-tap — any site could initiate credentialed
    // requests against the billing API. Now we require the dashboard
    // origin(s) to be declared explicitly (PUBLIC_BASE_URL + optional
    // CORS_ALLOWED_ORIGINS); preflight rejects everything else.
    tracing::info!(
        origins = ?cfg.cors_allowed_origins,
        "CORS allowlist"
    );
    let allowed = rocket_cors::AllowedOrigins::some_exact(&cfg.cors_allowed_origins);
    let cors = rocket_cors::CorsOptions {
        allowed_origins: allowed,
        allowed_methods: [
            rocket::http::Method::Get,
            rocket::http::Method::Post,
            rocket::http::Method::Put,
            rocket::http::Method::Options,
        ]
        .iter()
        .map(|m| rocket_cors::Method::from(*m))
        .collect(),
        allow_credentials: true,
        ..Default::default()
    }
    .to_cors()
    .expect("CORS failed to build");
    rocket::build()
        .attach(cors)
        .manage(pool)
        .manage(cfg)
        .manage(mailer)
        .manage(rate_limit)
        .manage(stripe)
        .manage(auth_resolver)
        .manage(per_customer_mutex)
        .mount(
            "/",
            routes![
                index,
                login_page,
                pay_with_litkey_page,
                health,
                auth_routes::request_link,
                auth_routes::verify_link,
                auth_routes::logout,
                auth_routes::me,
                portal_routes::lookup_customer,
                portal_routes::preview_customer,
                portal_routes::grant_credit,
                portal_routes::list_grants,
                rate::get_rate,
                rate::get_quote,
                chain::get_payment_config,
                chain::claim_payment,
                rate::override_rate,
                billing_routes::setup_intent::setup_intent,
                billing_routes::auto_topup_config::get_auto_topup_config,
                billing_routes::auto_topup_config::put_auto_topup_config,
                billing_routes::auto_topup_config::get_payment_method,
                billing_routes::sca_resume::get_auto_topup_resume,
                billing_routes::sca_resume::post_auto_topup_resume_complete,
                webhook_handler::stripe_webhook,
            ],
        )
        .mount("/static", FileServer::from("static"))
}

/// Translate platform-provided env vars (Fly.io and most container hosts set
/// `PORT`) into the names Rocket reads (`ROCKET_PORT`). Also default the bind
/// address to `0.0.0.0` so the container's listener is reachable through the
/// platform's ingress.
fn apply_platform_env() {
    if std::env::var("ROCKET_PORT").is_err()
        && let Ok(port) = std::env::var("PORT")
    {
        // SAFETY: invoked at startup before any threads are spawned.
        unsafe { std::env::set_var("ROCKET_PORT", port) };
    }
    if std::env::var("ROCKET_ADDRESS").is_err() {
        // SAFETY: invoked at startup before any threads are spawned.
        unsafe { std::env::set_var("ROCKET_ADDRESS", "0.0.0.0") };
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}

/// `GET /health` — always 200 OK once Rocket is up.
#[get("/health")]
fn health() -> &'static str {
    "ok"
}

/// `GET /` — serve the signed-in landing page if authed, else redirect to /login.
///
/// We don't gate on the Operator guard here (which would 401 / forward) so
/// that unauthed visitors get a clean redirect to /login.
#[get("/")]
async fn index(operator: Option<auth::Operator>) -> Result<NamedFile, Redirect> {
    match operator {
        Some(_) => NamedFile::open(static_path("index.html"))
            .await
            .map_err(|_| Redirect::to("/login?error=missing_static")),
        None => Err(Redirect::to("/login")),
    }
}

/// `GET /login` — serve the login page (always public).
#[get("/login")]
async fn login_page() -> Result<NamedFile, Status> {
    NamedFile::open(static_path("login.html"))
        .await
        .map_err(|_| Status::NotFound)
}

/// `GET /payWithLitkey` — serve the public LITKEY token payment page.
#[get("/payWithLitkey")]
async fn pay_with_litkey_page() -> Result<NamedFile, Status> {
    NamedFile::open(static_path("pay-with-litkey.html"))
        .await
        .map_err(|_| Status::NotFound)
}

fn static_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from("static");
    p.push(name);
    p
}
