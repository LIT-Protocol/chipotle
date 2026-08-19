//! OpenRouter integration (plans/tee-chat-app.md section 4.2).
//!
//! - Inference: SSE streaming proxy. Tokens are forwarded to the client as
//!   the completed message is buffered for encrypt-on-completion.
//! - Cost truth comes from OpenRouter's reported usage on the final chunk
//!   (`usage: {include: true}`), never a local price table.
//! - Provisioning: key mint / delete / credits via the provisioning key, so
//!   routine rotation is fully in-console.

use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct OpenRouterClient {
    http: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    /// OpenRouter-reported cost for the generation, in micro-USD.
    pub cost_micro_usd: i64,
}

/// Events forwarded from the upstream stream to the request handler.
#[derive(Debug)]
pub enum StreamEvent {
    /// Emitted once by the route handler before upstream starts: message ids
    /// the client should associate with this exchange.
    Meta(serde_json::Value),
    Delta(String),
    Done(Usage),
    Error(String),
}

impl OpenRouterClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            // No overall timeout: streams legitimately run for minutes.
            .build()
            .context("building OpenRouter HTTP client")?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// Stream a chat completion, sending events into `tx`. Consumes SSE from
    /// OpenRouter; emits Delta per content token, then exactly one Done (with
    /// usage) or Error. Returns the full accumulated assistant text.
    ///
    /// If `tx` closes (client went away / stop pressed), the upstream request
    /// is dropped — which cancels the generation — and the accumulated
    /// partial text is returned so the caller can persist it.
    pub async fn stream_chat(
        &self,
        api_key: &str,
        model: &str,
        messages: &[ChatMessage],
        max_tokens: i64,
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Result<(String, Option<Usage>, bool)> {
        let body = json!({
            "model": model,
            "messages": messages,
            "stream": true,
            "max_tokens": max_tokens,
            "usage": {"include": true},
        });
        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(api_key)
            .header("HTTP-Referer", "https://chat.litprotocol.com")
            .header("X-Title", "Lit Chat")
            .json(&body)
            .send()
            .await
            .context("OpenRouter request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            // Upstream error bodies can echo request content; log status only.
            tracing::warn!(%status, "OpenRouter returned non-success");
            let _ = tx
                .send(StreamEvent::Error(format!("upstream error ({status})")))
                .await;
            return Err(anyhow!(
                "OpenRouter error {status}: {}",
                truncate(&text, 200)
            ));
        }

        let mut full_text = String::new();
        let mut usage: Option<Usage> = None;
        let mut client_gone = false;
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();

        'outer: while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("OpenRouter stream read failed")?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            // SSE frames are separated by blank lines; process complete lines.
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim_end_matches('\r').to_string();
                buf.drain(..=pos);
                let Some(data) = line.strip_prefix("data: ") else {
                    continue; // comments (": OPENROUTER PROCESSING") and blanks
                };
                if data == "[DONE]" {
                    break 'outer;
                }
                let parsed: serde_json::Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if let Some(u) = parsed.get("usage").filter(|u| !u.is_null()) {
                    usage = Some(parse_usage(u));
                }
                let delta = parsed
                    .pointer("/choices/0/delta/content")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if !delta.is_empty() {
                    full_text.push_str(delta);
                    if !client_gone
                        && tx
                            .send(StreamEvent::Delta(delta.to_string()))
                            .await
                            .is_err()
                    {
                        // Client disconnected or pressed stop: cancel upstream
                        // by breaking out (drops the response stream) but keep
                        // what we have.
                        client_gone = true;
                        break 'outer;
                    }
                }
            }
        }

        if !client_gone {
            let _ = tx
                .send(StreamEvent::Done(usage.clone().unwrap_or_default()))
                .await;
        }
        Ok((full_text, usage, client_gone))
    }

    // -- Provisioning API (admin console) ------------------------------------

    pub async fn create_runtime_key(
        &self,
        provisioning_key: &str,
        name: &str,
        limit_usd: Option<f64>,
    ) -> Result<MintedKey> {
        let mut body = json!({"name": name});
        if let Some(l) = limit_usd {
            body["limit"] = json!(l);
        }
        let resp = self
            .http
            .post(format!("{}/keys", self.base_url))
            .bearer_auth(provisioning_key)
            .json(&body)
            .send()
            .await
            .context("OpenRouter key mint failed")?;
        let status = resp.status();
        let value: serde_json::Value =
            resp.json().await.context("OpenRouter key mint: bad json")?;
        if !status.is_success() {
            return Err(anyhow!("OpenRouter key mint error {status}"));
        }
        let key = value
            .get("key")
            .and_then(|v| v.as_str())
            .context("OpenRouter key mint: missing key")?
            .to_string();
        let hash = value
            .pointer("/data/hash")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        Ok(MintedKey { key, hash })
    }

    pub async fn delete_runtime_key(&self, provisioning_key: &str, hash: &str) -> Result<()> {
        let resp = self
            .http
            .delete(format!("{}/keys/{hash}", self.base_url))
            .bearer_auth(provisioning_key)
            .send()
            .await
            .context("OpenRouter key delete failed")?;
        if !resp.status().is_success() {
            return Err(anyhow!("OpenRouter key delete error {}", resp.status()));
        }
        Ok(())
    }

    pub async fn credits(&self, provisioning_key: &str) -> Result<Credits> {
        let resp = self
            .http
            .get(format!("{}/credits", self.base_url))
            .bearer_auth(provisioning_key)
            .send()
            .await
            .context("OpenRouter credits lookup failed")?;
        if !resp.status().is_success() {
            return Err(anyhow!("OpenRouter credits error {}", resp.status()));
        }
        let value: serde_json::Value = resp.json().await?;
        Ok(Credits {
            total_credits: value
                .pointer("/data/total_credits")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            total_usage: value
                .pointer("/data/total_usage")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
        })
    }

    /// Cheapest-model ping used by per-key health probes.
    pub async fn probe_key(&self, api_key: &str, model: &str) -> Result<()> {
        let body = json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1,
        });
        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .context("probe request failed")?;
        if !resp.status().is_success() {
            return Err(anyhow!("probe returned {}", resp.status()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MintedKey {
    /// Plaintext runtime key: encrypted into provider_keys immediately; never
    /// returned to a browser.
    pub key: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credits {
    pub total_credits: f64,
    pub total_usage: f64,
}

fn parse_usage(u: &serde_json::Value) -> Usage {
    let cost = u.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
    Usage {
        prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
        completion_tokens: u
            .get("completion_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        // Floor, never round up: rounding a single message up IS the
        // over-billing bug (section 8.2).
        cost_micro_usd: (cost * 1_000_000.0).floor() as i64,
    }
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_cost_floors_micro_usd() {
        let u = parse_usage(&json!({
            "prompt_tokens": 10, "completion_tokens": 20, "cost": 0.0000019
        }));
        assert_eq!(u.cost_micro_usd, 1);
        assert_eq!(u.prompt_tokens, 10);
        assert_eq!(u.completion_tokens, 20);
    }

    #[test]
    fn usage_missing_fields_default_zero() {
        let u = parse_usage(&json!({}));
        assert_eq!(u.cost_micro_usd, 0);
    }
}
