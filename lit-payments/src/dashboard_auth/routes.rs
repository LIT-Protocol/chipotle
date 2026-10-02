//! HTTP surface of the dashboard auth service: `/auth/v1/*`.
//!
//! Three catch-all Rocket routes (GET/POST/PUT) feed one dispatcher so the
//! request gate runs once, in a fixed order, for every path:
//!
//! 1. the feature must be configured (503 otherwise),
//! 2. `Origin` must exactly match a configured dashboard (403),
//! 3. `X-Chipotle-Auth: 1` must be present (403; it also forces a CORS
//!    preflight so plain forms can never reach these handlers),
//! 4. a per-client-IP limit,
//! 5. mutations must be JSON objects of at most 8 KiB,
//! 6. unknown paths 404 *before* any session is read,
//! 7. session-bound paths verify cookie, CSRF token and the `id`/`version`
//!    binding in the body.
//!
//! CORS headers and preflights are produced by the service-wide `rocket_cors`
//! fairing, whose allowlist includes the configured dashboard origins. Every
//! response is JSON with `Cache-Control: no-store`.

use std::io::Cursor;
use std::path::PathBuf;

use rocket::data::{Data, ToByteUnit};
use rocket::http::{ContentType, CookieJar, Method, Status};
use rocket::request::{FromRequest, Outcome, Request};
use rocket::response::{self, Responder, Response};
use rocket::{Route, State, get, post, put, routes};
use serde_json::{Value, json};
use sqlx::PgPool;

use super::db::{self, Session, User};
use super::protocol::{self, Parameters};
use super::{
    COOKIE, Dashboard, DashboardAuthConfig, Failure, MAX_BODY_BYTES, SESSION_IDLE_SECONDS,
    SESSION_MAX_SECONDS, SIGNUP_SESSION_IDLE_SECONDS, SIGNUP_SESSION_MAX_SECONDS, crypto, fail,
    now, outbox,
};
use crate::config::Config;
use crate::mail::Mailer;

pub fn routes() -> Vec<Route> {
    routes![get_route, post_route, put_route]
}

const SIGN_IN_AGAIN: &str = "Sign in again to continue.";
const ACCOUNT_CHANGED: &str = "Account changed. Sign in again.";
const SIGNUP_SESSION_REQUIRED: &str = "Signup session required.";
const WRONG_OPERATION: &str = "Wrong signup operation.";
const INVALID_RECORD: &str = "Invalid encrypted account record.";
const BAD_CREDENTIALS: &str = "Email or password is incorrect.";

/// Request facts captured by a guard so the dispatcher never touches the
/// `Request` directly.
pub struct Incoming {
    origin: Option<String>,
    client_header_ok: bool,
    csrf: Option<String>,
    cookie: Option<String>,
    ip: String,
    content_type_json: bool,
    content_length: Option<u64>,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Incoming {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let header = |name: &str| req.headers().get_one(name).map(str::to_string);
        let ip_header = req
            .rocket()
            .state::<Config>()
            .and_then(|c| c.dashboard_auth.as_ref())
            .map(|a| a.client_ip_header.clone())
            .unwrap_or_else(|| "X-Forwarded-For".to_string());
        // Last value = the hop appended by the trusted edge proxy. Earlier
        // values are client-controlled.
        let ip = header(&ip_header)
            .and_then(|v| v.rsplit(',').next().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .or_else(|| req.client_ip().map(|ip| ip.to_string()))
            .unwrap_or_else(|| "unknown".to_string());
        Outcome::Success(Incoming {
            origin: header("Origin"),
            client_header_ok: header("X-Chipotle-Auth").as_deref() == Some("1"),
            csrf: header("X-CSRF-Token"),
            cookie: req.cookies().get(COOKIE).map(|c| c.value().to_string()),
            ip,
            content_type_json: req.content_type().is_some_and(|ct| ct.is_json()),
            content_length: header("Content-Length").and_then(|v| v.parse().ok()),
        })
    }
}

/// JSON reply with the service's fixed response headers. Error statuses are
/// returned directly (never routed to catchers) so the body stays JSON.
pub struct Reply {
    status: u16,
    body: Value,
}

impl<'r> Responder<'r, 'static> for Reply {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        let body = self.body.to_string();
        Response::build()
            .status(Status::new(self.status))
            .header(ContentType::JSON)
            // Rocket's default Shield adds X-Content-Type-Options: nosniff.
            .raw_header("Cache-Control", "no-store")
            .sized_body(body.len(), Cursor::new(body))
            .ok()
    }
}

#[get("/auth/v1/<path..>")]
async fn get_route(
    path: PathBuf,
    incoming: Incoming,
    cookies: &CookieJar<'_>,
    cfg: &State<Config>,
    pool: &State<PgPool>,
    mailer: &State<Mailer>,
) -> Reply {
    dispatch(
        Method::Get,
        path,
        None,
        incoming,
        cookies,
        cfg,
        pool,
        mailer,
    )
    .await
}

#[post("/auth/v1/<path..>", data = "<data>")]
async fn post_route(
    path: PathBuf,
    data: Data<'_>,
    incoming: Incoming,
    cookies: &CookieJar<'_>,
    cfg: &State<Config>,
    pool: &State<PgPool>,
    mailer: &State<Mailer>,
) -> Reply {
    dispatch(
        Method::Post,
        path,
        Some(data),
        incoming,
        cookies,
        cfg,
        pool,
        mailer,
    )
    .await
}

#[put("/auth/v1/<path..>", data = "<data>")]
async fn put_route(
    path: PathBuf,
    data: Data<'_>,
    incoming: Incoming,
    cookies: &CookieJar<'_>,
    cfg: &State<Config>,
    pool: &State<PgPool>,
    mailer: &State<Mailer>,
) -> Reply {
    dispatch(
        Method::Put,
        path,
        Some(data),
        incoming,
        cookies,
        cfg,
        pool,
        mailer,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn dispatch(
    method: Method,
    path: PathBuf,
    data: Option<Data<'_>>,
    incoming: Incoming,
    cookies: &CookieJar<'_>,
    cfg: &Config,
    pool: &PgPool,
    mailer: &Mailer,
) -> Reply {
    let path = format!("/auth/v1/{}", path.to_string_lossy());
    match handle(method, &path, data, incoming, cookies, cfg, pool, mailer).await {
        Ok(body) => Reply { status: 200, body },
        Err(f) => Reply {
            status: f.status,
            body: json!({ "error": f.message }),
        },
    }
}

struct Ctx<'a> {
    cfg: &'a DashboardAuthConfig,
    dashboard: &'a Dashboard,
    pool: &'a PgPool,
    mailer: &'a Mailer,
    cookies: &'a CookieJar<'a>,
}

impl Ctx<'_> {
    fn parameters(&self, u: &User) -> Parameters {
        protocol::parameters(&self.cfg.environment, &u.id, &u.salt, u.version)
    }

    fn user_response(&self, u: &User, csrf: &str) -> Value {
        json!({
            "email": u.email,
            "parameters": self.parameters(u),
            "state": u.state,
            "operation": u.operation,
            "envelope": u.envelope.as_deref().map(|e| serde_json::from_str::<Value>(e).unwrap_or(Value::Null)),
            "csrf": csrf,
        })
    }

    fn set_cookie(&self, raw: String, max_age: i64) {
        self.cookies.add(self.cfg.session_cookie(raw, max_age));
    }

    fn clear_cookie(&self) {
        self.cookies.add(self.cfg.session_cookie(String::new(), 0));
    }

    /// Fire-and-forget delivery attempt for mail just queued.
    fn kick_outbox(&self) {
        tokio::spawn(outbox::deliver(
            self.pool.clone(),
            self.cfg.clone(),
            self.mailer.clone(),
        ));
    }

    async fn rate(&self, label: &str, max: i64, seconds: i64) -> Result<(), Failure> {
        let t = now();
        let key = crypto::mac_hex(&self.cfg.secret, &format!("rate:{}:{label}", t / seconds));
        let count = db::bump_limit(self.pool, &key, t + seconds * 2).await?;
        if count > max {
            return fail(429, "Too many attempts. Please try again later.");
        }
        Ok(())
    }

    /// Load the session and user for a cookie, enforcing expiry, idle
    /// timeout, credential version, and (for mutations) the CSRF token and
    /// the `id`/`version` binding in the body. Refreshes the idle timer.
    async fn session(
        &self,
        incoming: &Incoming,
        method: Method,
        body: &Value,
    ) -> Result<(Session, User), Failure> {
        let Some(raw) = incoming.cookie.as_deref().filter(|v| crypto::is_hex(v, 32)) else {
            return fail(401, SIGN_IN_AGAIN);
        };
        let hash = crypto::sha256_hex(raw);
        let Some(s) = db::live_session(self.pool, &hash, now()).await? else {
            return fail(401, SIGN_IN_AGAIN);
        };
        let Some(u) = db::user_by_id(self.pool, &s.user_id).await? else {
            return fail(401, SIGN_IN_AGAIN);
        };
        if u.version != s.version {
            return fail(401, SIGN_IN_AGAIN);
        }
        if method != Method::Get {
            if incoming.csrf.as_deref() != Some(s.csrf.as_str()) {
                return fail(403, "Refresh the page and try again.");
            }
            if body.get("id").and_then(Value::as_str) != Some(u.id.as_str())
                || body.get("version").and_then(Value::as_i64) != Some(u.version)
            {
                return fail(409, ACCOUNT_CHANGED);
            }
        }
        db::touch_session(self.pool, &hash, now() + SESSION_IDLE_SECONDS).await?;
        Ok((s, u))
    }

    /// Mint a session; fails if the credential changed since `u` was read.
    async fn issue(&self, u: &User, scope: &str) -> Result<(String, String), Failure> {
        let raw = crypto::random_hex(32);
        let hash = crypto::sha256_hex(&raw);
        let csrf = crypto::random_hex(32);
        let inserted = db::insert_session_if_current(
            self.pool,
            &hash,
            &csrf,
            scope,
            now() + SESSION_MAX_SECONDS,
            now() + SESSION_IDLE_SECONDS,
            &u.id,
            u.version,
            u.verifier.as_deref(),
        )
        .await?;
        if !inserted {
            return fail(409, ACCOUNT_CHANGED);
        }
        Ok((raw, csrf))
    }
}

/// Compare a submitted auth secret with the stored verifier in constant time.
fn reauthenticate(u: &User, secret: Option<&Value>) -> Result<(), Failure> {
    let candidate = crypto::verifier(&u.id, &protocol::token(secret)?);
    let stored = u.verifier.clone().unwrap_or_else(|| "0".repeat(64));
    if !crypto::constant_time_eq(&candidate, &stored) {
        return fail(401, BAD_CREDENTIALS);
    }
    Ok(())
}

/// The submitted `operation` must be a string equal to the stored one.
fn operation_matches(body: &Value, u: &User) -> bool {
    matches!(
        (body.get("operation").and_then(Value::as_str), u.operation.as_deref()),
        (Some(a), Some(b)) if a == b
    )
}

async fn read_body(
    method: Method,
    incoming: &Incoming,
    data: Option<Data<'_>>,
) -> Result<Value, Failure> {
    if method == Method::Get {
        return Ok(Value::Object(Default::default()));
    }
    if !incoming.content_type_json {
        return fail(415, "Use JSON.");
    }
    if incoming.content_length.unwrap_or(0) > MAX_BODY_BYTES as u64 {
        return fail(413, "Request too large.");
    }
    let Some(data) = data else {
        return fail(400, "Invalid JSON.");
    };
    // Bound streamed bodies too; Content-Length is not an authorization boundary.
    let bytes = match data.open((MAX_BODY_BYTES + 1).bytes()).into_bytes().await {
        Ok(bytes) => bytes,
        Err(_) => return fail(400, "Invalid JSON."),
    };
    if bytes.len() > MAX_BODY_BYTES {
        return fail(413, "Request too large.");
    }
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) if value.is_object() => Ok(value),
        _ => fail(400, "Invalid JSON."),
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle(
    method: Method,
    path: &str,
    data: Option<Data<'_>>,
    incoming: Incoming,
    cookies: &CookieJar<'_>,
    config: &Config,
    pool: &PgPool,
    mailer: &Mailer,
) -> Result<Value, Failure> {
    let Some(cfg) = config.dashboard_auth.as_ref() else {
        return fail(503, "Account service is not configured.");
    };
    let Some(dashboard) = incoming
        .origin
        .as_deref()
        .and_then(|o| cfg.dashboard_for_origin(o))
    else {
        return fail(403, "Origin not allowed.");
    };
    if !incoming.client_header_ok {
        return fail(403, "Invalid client request.");
    }
    let ctx = Ctx {
        cfg,
        dashboard,
        pool,
        mailer,
        cookies,
    };
    ctx.rate(&format!("ip:{}", incoming.ip), 120, 600).await?;
    let body = read_body(method, &incoming, data).await?;

    if path == "/auth/v1/signup/start" && method == Method::Post {
        let address = protocol::email(body.get("email"))?;
        ctx.rate(&format!("signup:{address}"), 3, 3600).await?;
        let existing = db::user_by_email(pool, &address).await?;
        let resumable = match &existing {
            None => true,
            Some(u) => u.state == "verified" && u.verifier.is_none(),
        };
        if resumable {
            outbox::queue_verification(pool, cfg, &ctx.dashboard.url, &address, "signup", None)
                .await?;
            ctx.kick_outbox();
        }
        return Ok(json!({
            "message": "If this email can create an account, a verification link is on its way. Existing users should sign in."
        }));
    }
    if path == "/auth/v1/signup/verify" && method == Method::Post {
        let hash = crypto::sha256_hex(&protocol::token(body.get("token"))?);
        let raw = crypto::random_hex(32);
        let claim = crypto::sha256_hex(&raw);
        let csrf = crypto::random_hex(32);
        let id = crypto::random_hex(16);
        let salt = crypto::random_hex(16);
        let t = now();
        let user = db::verify_signup(
            pool,
            &hash,
            &claim,
            &id,
            &salt,
            &csrf,
            t,
            t + SIGNUP_SESSION_MAX_SECONDS,
            t + SIGNUP_SESSION_IDLE_SECONDS,
        )
        .await?;
        let Some(u) = user else {
            return fail(
                400,
                "This verification link is expired or already used. Sign in or request a new link.",
            );
        };
        ctx.set_cookie(raw, SIGNUP_SESSION_MAX_SECONDS);
        return Ok(ctx.user_response(&u, &csrf));
    }
    if path == "/auth/v1/login/parameters" && method == Method::Post {
        let address = protocol::email(body.get("email"))?;
        ctx.rate(&format!("parameters:{address}"), 30, 600).await?;
        let parameters = match db::credentialed_user_by_email(pool, &address).await? {
            Some(u) => ctx.parameters(&u),
            // Deterministic synthetic parameters so unknown emails are indistinguishable.
            None => protocol::parameters(
                &cfg.environment,
                &crypto::synthetic_id(&cfg.secret, &address),
                &crypto::synthetic_salt(&cfg.secret, &address),
                1,
            ),
        };
        return Ok(serde_json::to_value(parameters).map_err(anyhow::Error::from)?);
    }
    if path == "/auth/v1/login" && method == Method::Post {
        let address = protocol::email(body.get("email"))?;
        ctx.rate(&format!("login:{address}"), 10, 600).await?;
        let user = db::user_by_email(pool, &address).await?;
        let id = match &user {
            Some(u) => u.id.clone(),
            None => crypto::synthetic_id(&cfg.secret, &address),
        };
        let candidate = crypto::verifier(&id, &protocol::token(body.get("authSecret"))?);
        let stored = user
            .as_ref()
            .and_then(|u| u.verifier.clone())
            .unwrap_or_else(|| "0".repeat(64));
        let authenticated = crypto::constant_time_eq(&candidate, &stored);
        let Some(u) = user.filter(|u| authenticated && u.verifier.is_some()) else {
            return fail(401, BAD_CREDENTIALS);
        };
        let scope = if u.state == "active" {
            "account"
        } else {
            "signup"
        };
        let (raw, csrf) = ctx.issue(&u, scope).await?;
        ctx.set_cookie(raw, SESSION_MAX_SECONDS);
        return Ok(ctx.user_response(&u, &csrf));
    }

    // Reject absent/nonexistent APIs, including reset/import, before touching sessions.
    const ALLOWED: [(Method, &str); 10] = [
        (Method::Get, "/auth/v1/session"),
        (Method::Post, "/auth/v1/logout"),
        (Method::Post, "/auth/v1/logout-all"),
        (Method::Post, "/auth/v1/signup/credentials"),
        (Method::Post, "/auth/v1/signup/begin"),
        (Method::Put, "/auth/v1/envelope"),
        (Method::Get, "/auth/v1/envelope"),
        (Method::Post, "/auth/v1/password/change"),
        (Method::Post, "/auth/v1/email/start"),
        (Method::Post, "/auth/v1/email/complete"),
    ];
    if !ALLOWED.contains(&(method, path)) {
        return fail(404, "Not found.");
    }
    let (s, u) = ctx.session(&incoming, method, &body).await?;

    if path == "/auth/v1/session" || (path == "/auth/v1/envelope" && method == Method::Get) {
        return Ok(ctx.user_response(&u, &s.csrf));
    }
    if path == "/auth/v1/logout" || path == "/auth/v1/logout-all" {
        if path.ends_with("logout-all") {
            db::delete_user_sessions(pool, &u.id).await?;
        } else {
            db::delete_session(pool, &s.hash).await?;
        }
        ctx.clear_cookie();
        return Ok(json!({ "ok": true }));
    }
    if path == "/auth/v1/signup/credentials" {
        if s.scope != "signup" {
            return fail(403, SIGNUP_SESSION_REQUIRED);
        }
        let value = crypto::verifier(&u.id, &protocol::token(body.get("authSecret"))?);
        if !db::set_credentials(pool, &u.id, u.version, &value, &crypto::random_hex(16)).await? {
            return fail(409, "Signup has already started. Sign in to continue.");
        }
        return Ok(json!({ "ok": true }));
    }
    if path == "/auth/v1/signup/begin" {
        if s.scope != "signup" {
            return fail(403, SIGNUP_SESSION_REQUIRED);
        }
        reauthenticate(&u, body.get("authSecret"))?;
        if body.get("retry") == Some(&Value::Bool(true)) {
            // The browser reports a known pre-creation rejection from the Lit API
            // for the attempt it names, so no account was created. Release that
            // claim under a fresh operation id; a stale tab cannot release a
            // newer attempt.
            if !operation_matches(&body, &u) {
                return fail(409, WRONG_OPERATION);
            }
            let operation = crypto::random_hex(16);
            if !db::release_creation(pool, &u, &operation).await? {
                return fail(409, "Account creation is not in a retryable state.");
            }
            return Ok(json!({ "operation": operation }));
        }
        if !db::claim_creation(pool, &u).await? {
            return fail(
                409,
                "Account creation already started. Do not create another account; resume saving the original key.",
            );
        }
        return Ok(json!({ "operation": u.operation }));
    }
    if path == "/auth/v1/envelope" && method == Method::Put {
        if s.scope != "signup" {
            return fail(403, SIGNUP_SESSION_REQUIRED);
        }
        reauthenticate(&u, body.get("authSecret"))?;
        let envelope = match protocol::validate_envelope(
            body.get("envelope").unwrap_or(&Value::Null),
            &ctx.parameters(&u),
        ) {
            Ok(e) => e,
            Err(_) => return fail(400, INVALID_RECORD),
        };
        if !operation_matches(&body, &u) {
            return fail(409, WRONG_OPERATION);
        }
        let encoded = serde_json::to_string(&envelope).map_err(anyhow::Error::from)?;
        if u.state == "active" {
            if u.envelope.as_deref() != Some(encoded.as_str()) {
                return fail(409, "Account already created.");
            }
            return Ok(json!({ "ok": true }));
        }
        let operation = body
            .get("operation")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !db::activate(pool, &u, &envelope.account, &encoded, operation).await? {
            return fail(409, ACCOUNT_CHANGED);
        }
        return Ok(json!({ "ok": true }));
    }
    if u.state != "active" || s.scope != "account" {
        return fail(403, "Sign in to your completed account first.");
    }
    if path == "/auth/v1/password/change" {
        reauthenticate(&u, body.get("oldAuthSecret"))?;
        let new_secret = protocol::token(body.get("authSecret"))?;
        let mut p = ctx.parameters(&u);
        p.version = u.version + 1;
        p.salt = body
            .get("envelope")
            .and_then(|e| e.get("salt"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let envelope =
            match protocol::validate_envelope(body.get("envelope").unwrap_or(&Value::Null), &p) {
                Ok(e) => e,
                Err(_) => return fail(400, INVALID_RECORD),
            };
        let current_nonce = u
            .envelope
            .as_deref()
            .and_then(|e| serde_json::from_str::<Value>(e).ok())
            .and_then(|e| e.get("nonce").and_then(Value::as_str).map(str::to_string));
        if Some(envelope.account.as_str()) != u.account.as_deref()
            || envelope.salt == u.salt
            || Some(envelope.nonce.as_str()) == current_nonce.as_deref()
        {
            return fail(
                400,
                "Password changes require fresh encryption parameters and the same account.",
            );
        }
        let new_verifier = crypto::verifier(&u.id, &new_secret);
        let notice = outbox::notification(
            cfg,
            &u.email,
            "Chipotle password changed",
            "Your Chipotle password was changed. Existing exported API keys remain valid. If this was not you, review your account access.",
        )?;
        let encoded = serde_json::to_string(&envelope).map_err(anyhow::Error::from)?;
        if !db::change_password(pool, &u, &new_verifier, &envelope.salt, &encoded, &notice).await? {
            return fail(409, ACCOUNT_CHANGED);
        }
        ctx.kick_outbox();
        // The version bump already invalidated sessions atomically; this
        // cleanup cannot revoke newer sessions.
        let mut conn = pool.acquire().await?;
        db::delete_stale_credentials(&mut conn, &u.id, u.version).await?;
        ctx.clear_cookie();
        return Ok(json!({ "ok": true }));
    }
    if path == "/auth/v1/email/start" {
        reauthenticate(&u, body.get("authSecret"))?;
        let address = protocol::email(body.get("email"))?;
        ctx.rate(&format!("email:{}", u.id), 3, 3600).await?;
        if !db::email_taken(pool, &address).await? {
            outbox::queue_verification(pool, cfg, &ctx.dashboard.url, &address, "email", Some(&u))
                .await?;
            ctx.kick_outbox();
        }
        return Ok(
            json!({ "message": "If this address is available, a verification link is on its way." }),
        );
    }
    if path == "/auth/v1/email/complete" {
        reauthenticate(&u, body.get("authSecret"))?;
        let hash = crypto::sha256_hex(&protocol::token(body.get("token"))?);
        let claim = crypto::random_hex(32);
        // Fail early with a clear status when the verified address was registered
        // by someone else after the link was sent; the transaction below still
        // guards the race (its UNIQUE violation surfaces as a generic failure).
        if db::email_proof_taken(pool, &u.id, &hash).await? {
            return fail(
                409,
                "That email address is already in use by another account.",
            );
        }
        let notice = outbox::notification(
            cfg,
            &u.email,
            "Chipotle email changed",
            "Your Chipotle sign-in email was changed. Your password and API key have not changed. If this was not you, review your account access.",
        )?;
        if !db::complete_email_change(pool, &u, &hash, &claim, now(), &notice).await? {
            return fail(400, "Email verification link is expired or already used.");
        }
        ctx.kick_outbox();
        ctx.clear_cookie();
        return Ok(json!({ "ok": true }));
    }
    fail(404, "Not found.")
}
