use lit_bridge::config;
use rocket::fs::{FileServer, NamedFile};
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{get, routes, State};
use std::path::PathBuf;

#[rocket::launch]
async fn rocket() -> _ {
    init_tracing();
    apply_platform_env();
    let cfg = config::Config::from_env().expect("config");

    rocket::build()
        .manage(cfg)
        .mount("/", routes![index, health, api_config])
        .mount("/static", FileServer::from("static"))
}

/// Translate the platform-provided `PORT` into Rocket's `ROCKET_PORT`, and bind
/// `0.0.0.0` so the container's listener is reachable through ingress.
fn apply_platform_env() {
    if std::env::var("ROCKET_PORT").is_err() {
        if let Ok(port) = std::env::var("PORT") {
            // SAFETY: invoked at startup before any threads are spawned.
            unsafe { std::env::set_var("ROCKET_PORT", port) };
        }
    }
    if std::env::var("ROCKET_ADDRESS").is_err() {
        // SAFETY: invoked at startup before any threads are spawned.
        unsafe { std::env::set_var("ROCKET_ADDRESS", "0.0.0.0") };
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();
}

/// `GET /health` — always 200 OK once Rocket is up (Railway healthcheck).
#[get("/health")]
fn health() -> &'static str {
    "ok"
}

/// `GET /api/config` — public bootstrap config for the UI (where the registry
/// lives). No secrets.
#[get("/api/config")]
fn api_config(cfg: &State<config::Config>) -> Json<config::Config> {
    Json(cfg.inner().clone())
}

/// `GET /` — the bridging UI. Public (no auth gate).
#[get("/")]
async fn index() -> Result<NamedFile, Status> {
    NamedFile::open(static_path("index.html"))
        .await
        .map_err(|_| Status::NotFound)
}

fn static_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from("static");
    p.push(name);
    p
}
