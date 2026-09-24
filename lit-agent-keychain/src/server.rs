use crate::{
    api, auth, billing, chipotle::Chipotle, config::Config, registry, sponsorship, stripe::Stripe,
    subscriptions, templates,
};
use rocket::{
    catch, catchers,
    data::{Limits, ToByteUnit},
    fairing::{Fairing, Info, Kind},
    fs::{FileServer, NamedFile},
    get,
    http::{Header, Status},
    routes,
    serde::json::Json,
    Build, Request, Response, Rocket, State,
};
use serde_json::{json, Value};
use sqlx::PgPool;

pub fn build(cfg: Config, pool: PgPool, lit: Chipotle, stripe: Stripe) -> Rocket<Build> {
    let web_dir = cfg.web_dir.clone();
    let figment =
        rocket::Config::figment().merge(("limits", Limits::new().limit("json", 256.kibibytes())));
    rocket::custom(figment)
        .manage(cfg)
        .manage(pool)
        .manage(lit)
        .manage(stripe)
        .attach(Headers)
        .mount(
            "/",
            routes![
                health,
                crate::discovery::challenge,
                crate::discovery::discover,
                config,
                index,
                auth::challenge,
                auth::passkey_lookup,
                auth::login,
                auth::logout,
                auth::me,
                templates::index,
                templates::template,
                billing::execute,
                subscriptions::status,
                subscriptions::refresh_route,
                subscriptions::checkout,
                subscriptions::portal,
                subscriptions::webhook,
                sponsorship::key,
                sponsorship::rotate,
                sponsorship::enroll,
                sponsorship::prepare,
                registry::credentials,
                registry::restore_credentials,
                registry::policy,
                registry::update_credentials,
                registry::create,
                registry::restore,
                registry::list,
                registry::bundle,
                registry::update_policy,
                registry::rotate,
                registry::delete_secret,
                registry::audit_log
            ],
        )
        .mount("/", FileServer::from(web_dir).rank(20))
        .register("/", catchers![errors])
}
#[get("/health")]
fn health() -> &'static str {
    "ok"
}
#[get("/api/config")]
fn config(cfg: &State<Config>) -> Json<Value> {
    Json(
        json!({"protocol":2,"network":cfg.network,"registry":cfg.public_base_url,"googleClientId":cfg.google_client_id,
    "maxSecretBytes":16384,"maxPolicyDays":Value::Null,"revocationTrust":"operator_can_replay_prior_signed_permissions",
    "pricing":{"priceCents":1000,"currency":"usd","interval":"month","secretLimit":1000,"freeSecretLimit":5,"contactEmail":cfg.contact_email}}),
    )
}
#[get("/")]
async fn index(cfg: &State<Config>) -> Result<NamedFile, Status> {
    NamedFile::open(std::path::Path::new(&cfg.web_dir).join("index.html"))
        .await
        .map_err(|_| Status::NotFound)
}
#[catch(default)]
fn errors(status: Status, _req: &Request<'_>) -> Json<api::ErrorResponse> {
    Json(api::ErrorResponse {
        error: status
            .reason()
            .unwrap_or("error")
            .to_lowercase()
            .replace(' ', "_"),
    })
}
struct Headers;
#[rocket::async_trait]
impl Fairing for Headers {
    fn info(&self) -> Info {
        Info {
            name: "Keychain response protections",
            kind: Kind::Response,
        }
    }
    async fn on_response<'r>(&self, req: &'r Request<'_>, res: &mut Response<'r>) {
        res.set_header(Header::new("X-Content-Type-Options", "nosniff"));
        res.set_header(Header::new(
            "Referrer-Policy",
            "strict-origin-when-cross-origin",
        ));
        res.set_header(Header::new("X-Frame-Options", "DENY"));
        // GIS needs its exact Google script/frame origin. Wallet RPC/relay URLs
        // are provider-specific; the action bundles remain entirely local.
        res.set_header(Header::new("Content-Security-Policy","default-src 'self'; script-src 'self' https://accounts.google.com/gsi/client; style-src 'self' 'unsafe-inline' https://accounts.google.com; frame-src https://accounts.google.com https://verify.walletconnect.com https://verify.walletconnect.org; connect-src 'self' https: wss:; img-src 'self' data: https:; font-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"));
        if let Some(cfg) = req.rocket().state::<Config>() {
            if let Some(csp) = res.headers().get_one("Content-Security-Policy") {
                let csp = csp.replace(
                    "connect-src 'self' https: wss:",
                    &format!("connect-src 'self' https: wss: {}", cfg.lit_api_url),
                );
                res.set_header(Header::new("Content-Security-Policy", csp));
            }
        }
        if req.uri().path().starts_with("/api/") || req.uri().path().starts_with("/auth/") {
            res.set_header(Header::new("Cache-Control", "no-store"));
        }
    }
}
