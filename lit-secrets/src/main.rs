use std::path::PathBuf;

use anyhow::Result;
use lit_secrets::auth::routes as auth_routes;
use lit_secrets::{
    actions, agents, audit, auth, chipotle, config, db, grants, mail, secrets, signer, tenants,
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
    let chipotle =
        chipotle::ChipotleClient::new(cfg.chipotle_api_base_url.clone()).expect("chipotle client");
    let grant_signer =
        signer::GrantSigner::from_hex(&cfg.grant_signing_key).expect("GRANT_SIGNING_KEY");
    let action_set = actions::ActionSet::build(&grant_signer.address());
    tracing::info!(
        grant_signer = %action_set.grant_signer,
        reader_cid = %action_set.reader_cid,
        encrypt_cid = %action_set.encrypt_cid,
        chipotle = %cfg.chipotle_api_base_url,
        "lit-secrets starting"
    );

    rocket::build()
        .manage(pool)
        .manage(cfg)
        .manage(mailer)
        .manage(rate_limit)
        .manage(chipotle)
        .manage(grant_signer)
        .manage(action_set)
        .manage(tenants::ProvisionLock::default())
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
                tenants::get_tenant,
                tenants::provision_tenant,
                tenants::add_tenant_action,
                tenants::list_tenant_actions,
                tenants::remove_tenant_action,
                secrets::create_secret,
                secrets::list_secrets,
                secrets::get_secret,
                secrets::rotate_secret,
                secrets::update_secret,
                secrets::delete_secret,
                agents::create_agent,
                agents::list_agents,
                agents::revoke_agent,
                grants::issue_grant,
                grants::get_reference,
                audit::list_audit,
            ],
        )
        .mount("/static", FileServer::from("static"))
        .mount("/sdk", FileServer::from("sdk"))
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
