//! Outbound email via Resend (https://resend.com/docs/api-reference/emails/send-email).

use anyhow::{Context, Result};
use serde::Serialize;

pub const DEFAULT_RESEND_API_BASE_URL: &str = "https://api.resend.com";

#[derive(Clone)]
pub struct Mailer {
    // All fields are Clone (reqwest::Client clones cheaply, String clones the
    // underlying buffer). Used by tokio::spawn in /auth/request so the email
    // send runs off the request path.
    api_key: String,
    from: String,
    /// Resend API base URL. Overridable (`RESEND_API_BASE_URL`) so the test
    // harness can capture mail with a local fake; production uses the default.
    base_url: String,
    http: reqwest::Client,
}

/// Why a queued send failed: a provider HTTP rejection (status kept for
/// operational logs; the body is never logged) or a transport error.
#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("Resend returned HTTP {0}")]
    Http(u16),
    #[error("Resend request failed")]
    Transport,
}

impl MailError {
    pub fn status(&self) -> Option<u16> {
        match self {
            MailError::Http(s) => Some(*s),
            MailError::Transport => None,
        }
    }
}

impl Mailer {
    pub fn new(api_key: String, from: String, base_url: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .context("building Resend HTTP client")?;
        Ok(Self {
            api_key,
            from,
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/emails", self.base_url)
    }

    /// Send a single email. Returns `Ok(())` on 2xx, `Err` otherwise.
    pub async fn send(&self, to: &str, subject: &str, html: &str, text: &str) -> Result<()> {
        let req = ResendSendRequest {
            from: &self.from,
            to: &[to],
            subject,
            html,
            text,
        };
        let resp = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
            .json(&req)
            .send()
            .await
            .context("Resend HTTP request failed")?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Resend returned {status}: {body}");
        }
        Ok(())
    }

    /// Plain-text send with a caller-supplied idempotency key, used by the
    /// dashboard-auth outbox so retries never duplicate a verification email.
    pub async fn send_queued(
        &self,
        to: &str,
        subject: &str,
        text: &str,
        idempotency_key: &str,
    ) -> std::result::Result<(), MailError> {
        let req = ResendTextRequest {
            from: &self.from,
            to,
            subject,
            text,
        };
        let resp = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
            .header("Idempotency-Key", idempotency_key)
            .json(&req)
            .send()
            .await
            .map_err(|_| MailError::Transport)?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(MailError::Http(status.as_u16()))
        }
    }
}

#[derive(Serialize)]
struct ResendSendRequest<'a> {
    from: &'a str,
    to: &'a [&'a str],
    subject: &'a str,
    html: &'a str,
    text: &'a str,
}

#[derive(Serialize)]
struct ResendTextRequest<'a> {
    from: &'a str,
    to: &'a str,
    subject: &'a str,
    text: &'a str,
}
