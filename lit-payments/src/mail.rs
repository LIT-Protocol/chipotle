//! Outbound email via Resend (https://resend.com/docs/api-reference/emails/send-email).

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Clone)]
pub struct Mailer {
    api_key: String,
    from: String,
    http: reqwest::Client,
}

impl Mailer {
    pub fn new(api_key: String, from: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .context("building Resend HTTP client")?;
        Ok(Self {
            api_key,
            from,
            http,
        })
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
            .post("https://api.resend.com/emails")
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
}

#[derive(Serialize)]
struct ResendSendRequest<'a> {
    from: &'a str,
    to: &'a [&'a str],
    subject: &'a str,
    html: &'a str,
    text: &'a str,
}
