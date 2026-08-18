//! Magic-link auth routes: request, verify, logout.

use rocket::State;
use rocket::form::Form;
use rocket::http::{Cookie, CookieJar, SameSite, Status};
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::{get, post};
use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;

use super::operator::Operator;
use super::rate_limit::RateLimiter;
use super::{MAGIC_LINK_TTL_SECONDS, SESSION_COOKIE_NAME, operator, session, token, used_link};
use crate::config::Config;
use crate::mail::Mailer;

#[derive(rocket::FromForm)]
pub struct RequestLinkForm<'r> {
    pub email: &'r str,
}

#[derive(Serialize)]
pub struct RequestLinkResponse {
    /// Always true (to prevent email enumeration). The client should display
    /// "If your email is on the allowlist, a link is on its way."
    pub ok: bool,
}

/// `POST /auth/request` — accept any email and, IF it matches an allowlisted
/// operator, send them a magic-link email. Otherwise no-op.
///
/// Same JSON shape regardless of allowlist membership so callers can't
/// enumerate operators. Two related defenses:
///
/// 1. **Rate-limit check runs first**, before the operators table is touched,
///    so a flood of requests for any single email returns at constant time
///    regardless of allowlist status (defeats inbox spam + Resend-quota
///    burning).
/// 2. **Email send is spawned**, so the operator branch returns at the same
///    speed as the non-operator branch (defeats latency-based enumeration of
///    the operators table).
#[post("/auth/request", data = "<form>")]
pub async fn request_link(
    form: Form<RequestLinkForm<'_>>,
    pool: &State<PgPool>,
    config: &State<Config>,
    mailer: &State<Mailer>,
    rate_limit: &State<RateLimiter>,
) -> Json<RequestLinkResponse> {
    let email = form.email.trim().to_lowercase();

    if rate_limit.check_and_record(&email).await {
        tracing::info!("magic-link request rate-limited for {email}");
        return Json(RequestLinkResponse { ok: true });
    }

    match operator::find_by_email(pool, &email).await {
        Ok(Some(_op)) => {
            let now = OffsetDateTime::now_utc().unix_timestamp();
            let expires = now + MAGIC_LINK_TTL_SECONDS;
            let tok = token::issue(&config.magic_link_signing_key, &email, expires);
            let link = format!("{}/auth/verify?token={tok}", config.public_base_url);
            let subject = "Sign in to Lit Payments";
            let text = format!(
                "Click to sign in to Lit Payments (link expires in 15 minutes):\n\n{link}\n\nIf you didn't request this, you can ignore this email."
            );
            let html = format!(
                "<p>Click to sign in to Lit Payments (link expires in 15 minutes):</p>\
                 <p><a href=\"{link}\">{link}</a></p>\
                 <p style=\"color: #777; font-size: 12px;\">If you didn't request this, you can ignore this email.</p>"
            );

            let mailer = mailer.inner().clone();
            let email_for_send = email.clone();
            tokio::spawn(async move {
                if let Err(e) = mailer.send(&email_for_send, subject, &html, &text).await {
                    tracing::warn!("magic-link email send failed for {email_for_send}: {e}");
                } else {
                    tracing::info!("magic-link sent to {email_for_send}");
                }
            });
        }
        Ok(None) => {
            tracing::info!("magic-link requested for non-operator email; ignoring");
        }
        Err(e) => {
            tracing::warn!("operator lookup failed in /auth/request: {e}");
        }
    }
    Json(RequestLinkResponse { ok: true })
}

/// `GET /auth/verify?token=...` — validate the magic-link token, create a
/// session, set the cookie, and redirect to `/`. On any failure redirect to
/// `/login?error=invalid`.
#[get("/auth/verify?<token>")]
pub async fn verify_link(
    token: &str,
    pool: &State<PgPool>,
    config: &State<Config>,
    cookies: &CookieJar<'_>,
) -> Result<Redirect, Redirect> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let claims = match token::verify(&config.magic_link_signing_key, token, now) {
        Ok(c) => c,
        Err(e) => {
            tracing::info!("magic-link verify rejected: {e}");
            return Err(Redirect::to("/login?error=invalid"));
        }
    };

    let op = match operator::find_by_email(pool, &claims.email).await {
        Ok(Some(op)) => op,
        Ok(None) => {
            // Token valid but email removed from operators in the meantime.
            tracing::info!(
                "magic-link verify: valid token for non-operator email {}",
                claims.email
            );
            return Err(Redirect::to("/login?error=invalid"));
        }
        Err(e) => {
            tracing::warn!("operator lookup failed in /auth/verify: {e}");
            return Err(Redirect::to("/login?error=server"));
        }
    };

    // Enforce single use (CPL-379 L8). The token is a stateless HMAC blob and
    // would otherwise be replayable for its full TTL. Burn it now that we've
    // confirmed a valid token for a real operator: `try_consume` returns false
    // if it was already redeemed. Concurrent redemptions of the same token race
    // safely on the table's primary key — only one wins.
    let token_hash = token::token_hash(token);
    let expires_at = OffsetDateTime::from_unix_timestamp(claims.expires_unix)
        .unwrap_or_else(|_| OffsetDateTime::now_utc());
    match used_link::try_consume(pool, &token_hash, &claims.email, expires_at).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::info!("magic-link verify rejected: token already used");
            return Err(Redirect::to("/login?error=invalid"));
        }
        Err(e) => {
            tracing::warn!("magic-link single-use check failed: {e}");
            return Err(Redirect::to("/login?error=server"));
        }
    }

    let session_token = session::generate_token();
    if let Err(e) = session::create(pool, &session_token, op.id).await {
        tracing::warn!("session create failed for operator {}: {e}", op.id);
        return Err(Redirect::to("/login?error=server"));
    }
    operator::touch_last_login(pool, op.id).await;

    let cookie = Cookie::build((SESSION_COOKIE_NAME, session_token))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .path("/")
        .build();
    cookies.add_private(cookie);

    Ok(Redirect::to("/"))
}

/// `POST /auth/logout` — delete the current session row and clear the cookie.
/// Idempotent.
#[post("/auth/logout")]
pub async fn logout(pool: &State<PgPool>, cookies: &CookieJar<'_>) -> Status {
    if let Some(cookie) = cookies.get_private(SESSION_COOKIE_NAME) {
        let token = cookie.value().to_string();
        if let Err(e) = session::delete(pool, &token).await {
            tracing::warn!("session delete failed: {e}");
        }
        cookies.remove_private(SESSION_COOKIE_NAME);
    }
    Status::NoContent
}

/// `GET /api/me` — returns the current operator profile, or 401 if not
/// signed in. The login UI polls this to render "signed in as …".
#[get("/api/me")]
pub fn me(operator: Operator) -> Json<Operator> {
    Json(operator)
}
