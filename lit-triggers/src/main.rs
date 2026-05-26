use std::path::PathBuf;

use anyhow::Result;
use lit_triggers::auth::routes as auth_routes;
use lit_triggers::{
    auth, chain_events, config, db, dispatcher, mail, scheduler, triggers, webhook,
    webhook_rate_limit,
};
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

    if let Err(e) = auth::session::purge_expired(&pool).await {
        tracing::warn!("session purge on boot failed: {e}");
    }

    let mailer =
        mail::Mailer::new(cfg.resend_api_key.clone(), cfg.mail_from.clone()).expect("mailer");
    let rate_limit = auth::rate_limit::RateLimiter::new();
    let webhook_rate_limit = webhook_rate_limit::WebhookRateLimiter::new();

    tokio::spawn(dispatcher::run(pool.clone(), cfg.clone()));
    tokio::spawn(scheduler::run(pool.clone(), cfg.clone()));
    tokio::spawn(chain_events::run(pool.clone(), cfg.clone()));

    rocket::build()
        .manage(pool)
        .manage(cfg)
        .manage(mailer)
        .manage(rate_limit)
        .manage(webhook_rate_limit)
        .mount(
            "/",
            routes![
                index,
                login_page,
                health,
                skill_doc,
                agent_authorize_page,
                auth_routes::request_link,
                auth_routes::verify_link,
                auth_routes::logout,
                auth_routes::authorize_agent,
                auth_routes::me,
                triggers::create_trigger,
                triggers::list_triggers,
                triggers::get_trigger,
                triggers::update_trigger,
                triggers::delete_trigger,
                triggers::list_runs,
                triggers::test_trigger,
                webhook::receive_webhook,
            ],
        )
        .mount("/static", FileServer::from("static"))
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

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}

#[get("/health")]
fn health() -> &'static str {
    "ok"
}

#[get("/SKILL.md")]
async fn skill_doc() -> Result<NamedFile, Status> {
    NamedFile::open("SKILL.md")
        .await
        .map_err(|_| Status::NotFound)
}

#[get("/agent/authorize?<challenge>")]
async fn agent_authorize_page(
    user: Option<auth::User>,
    challenge: Option<&str>,
) -> Result<NamedFile, Redirect> {
    let Some(challenge) = challenge else {
        return Err(Redirect::to("/login?error=invalid"));
    };
    if auth::agent::validate_agent_token_hash(challenge).is_err() {
        return Err(Redirect::to("/login?error=invalid"));
    }
    match user {
        Some(_) => NamedFile::open(static_path("agent-authorize.html"))
            .await
            .map_err(|_| Redirect::to("/login?error=missing_static")),
        None => Err(Redirect::to(format!(
            "/login?next=/agent/authorize%3Fchallenge%3D{challenge}"
        ))),
    }
}

#[get("/")]
async fn index(user: Option<auth::User>) -> Result<NamedFile, Redirect> {
    match user {
        Some(_) => NamedFile::open(static_path("index.html"))
            .await
            .map_err(|_| Redirect::to("/login?error=missing_static")),
        None => Err(Redirect::to("/login")),
    }
}

#[get("/login")]
async fn login_page() -> Result<NamedFile, Status> {
    NamedFile::open(static_path("login.html"))
        .await
        .map_err(|_| Status::NotFound)
}

fn static_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from("static");
    p.push(name);
    p
}
