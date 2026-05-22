//! Magic-link auth routes for public lit-triggers users.

use rocket::form::Form;
use rocket::http::{Cookie, CookieJar, SameSite, Status};
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::State;
use rocket::{get, post};
use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;

use super::rate_limit::RateLimiter;
use super::user::User;
use super::{session, token, user, MAGIC_LINK_TTL_SECONDS, SESSION_COOKIE_NAME};
use crate::config::Config;
use crate::mail::Mailer;

#[derive(rocket::FromForm)]
pub struct RequestLinkForm<'r> {
    pub email: &'r str,
}

#[derive(Serialize)]
pub struct RequestLinkResponse {
    pub ok: bool,
}

#[post("/auth/request", data = "<form>")]
pub async fn request_link(
    form: Form<RequestLinkForm<'_>>,
    pool: &State<PgPool>,
    config: &State<Config>,
    mailer: &State<Mailer>,
    rate_limit: &State<RateLimiter>,
) -> Json<RequestLinkResponse> {
    let email = form.email.trim().to_lowercase();
    if email.is_empty() || rate_limit.check_and_record(&email).await {
        return Json(RequestLinkResponse { ok: true });
    }

    let now = OffsetDateTime::now_utc();
    let expires_at = now + time::Duration::seconds(MAGIC_LINK_TTL_SECONDS);
    let nonce = token::generate_nonce();
    let tok = token::issue(
        &config.magic_link_signing_key,
        &email,
        expires_at.unix_timestamp(),
        &nonce,
    );
    match store_magic_link(pool, &tok, &nonce, &email, expires_at).await {
        Ok(()) => {
            let link = format!("{}/auth/verify?token={tok}", config.public_base_url);
            let subject = "Sign in to Lit Triggers";
            let text = format!(
                "Click to sign in to Lit Triggers (link expires in 15 minutes):\n\n{link}\n\nIf you didn't request this, you can ignore this email."
            );
            let html = format!(
                "<p>Click to sign in to Lit Triggers (link expires in 15 minutes):</p>\
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
        Err(e) => tracing::warn!("magic-link store failed in /auth/request: {e}"),
    }
    Json(RequestLinkResponse { ok: true })
}

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

    if claims.nonce.is_empty() {
        tracing::info!("magic-link verify rejected: empty nonce");
        return Err(Redirect::to("/login?error=invalid"));
    }

    let consumed = match consume_magic_link(pool, token, &claims.nonce, &claims.email).await {
        Ok(consumed) => consumed,
        Err(e) => {
            tracing::warn!("magic-link consume failed in /auth/verify: {e}");
            return Err(Redirect::to("/login?error=server"));
        }
    };
    if !consumed {
        tracing::info!("magic-link verify rejected: missing, expired, or already consumed token");
        return Err(Redirect::to("/login?error=invalid"));
    }

    let u = match user::find_or_create_by_email(pool, &claims.email).await {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!("user create/lookup failed in /auth/verify: {e}");
            return Err(Redirect::to("/login?error=server"));
        }
    };

    let session_token = session::generate_token();
    if let Err(e) = session::create(pool, &session_token, u.id).await {
        tracing::warn!("session create failed for user {}: {e}", u.id);
        return Err(Redirect::to("/login?error=server"));
    }
    user::touch_last_login(pool, u.id).await;

    let cookie = Cookie::build((SESSION_COOKIE_NAME, session_token))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .path("/")
        .build();
    cookies.add_private(cookie);

    Ok(Redirect::to("/"))
}

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

#[get("/api/me")]
pub fn me(user: User) -> Json<User> {
    Json(user)
}

async fn store_magic_link(
    pool: &PgPool,
    raw_token: &str,
    nonce: &str,
    email: &str,
    expires_at: OffsetDateTime,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO magic_links (token_hash, nonce, email, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(token::token_hash(raw_token))
    .bind(nonce)
    .bind(email)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

async fn consume_magic_link(
    pool: &PgPool,
    raw_token: &str,
    nonce: &str,
    email: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE magic_links
         SET consumed_at = now()
         WHERE token_hash = $1
           AND nonce = $2
           AND email = $3
           AND consumed_at IS NULL
           AND expires_at > now()",
    )
    .bind(token::token_hash(raw_token))
    .bind(nonce)
    .bind(email)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}
