//! Magic-link mail via Resend (lit-triggers pattern). Content-free logging:
//! the recipient address is never logged.

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
        if !status.is_success() {
            // Do not log the body: it echoes the recipient address.
            anyhow::bail!("Resend returned {status}");
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

pub fn code_email(code: &str) -> (String, String, String) {
    let subject = "Your Lit Chat sign-in code".to_string();
    let text = format!(
        "Your sign-in code is: {code}\n\nEnter it in the tab where you requested it. \
         It expires in 15 minutes and works once.\n\nIf you didn't request this, ignore this email."
    );
    let html = format!(
        "<p>Your sign-in code is:</p>\
         <p style=\"font-size:24px;font-family:monospace;letter-spacing:2px\"><b>{code}</b></p>\
         <p>Enter it in the tab where you requested it. It expires in 15 minutes and works once.</p>\
         <p>If you didn't request this, ignore this email.</p>"
    );
    (subject, html, text)
}
