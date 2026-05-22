//! Chipotle Lit Action client used by the async run dispatcher.

use anyhow::Result;
use reqwest::StatusCode;
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone)]
pub struct ChipotleClient {
    http: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct LitActionRequest {
    pub code: String,
    pub js_params: Value,
}

#[derive(Debug)]
pub struct ChipotleError {
    pub message: String,
    pub transient: bool,
    pub response: Option<Value>,
}

impl std::fmt::Display for ChipotleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ChipotleError {}

impl ChipotleClient {
    pub fn new(base_url: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Chipotle HTTP client configuration is valid");
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub fn lit_action_url(&self) -> String {
        format!("{}/lit_action", self.base_url)
    }

    pub fn build_lit_action_request(action_code: String, params: Value) -> LitActionRequest {
        LitActionRequest {
            code: action_code,
            js_params: params,
        }
    }

    pub async fn execute_lit_action(
        &self,
        usage_api_key: &str,
        action_code: String,
        params: Value,
    ) -> Result<Value, ChipotleError> {
        let body = Self::build_lit_action_request(action_code, params);
        let response = self
            .http
            .post(self.lit_action_url())
            .bearer_auth(usage_api_key)
            .header("X-Api-Key", usage_api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ChipotleError {
                message: format!("chipotle request failed: {e}"),
                transient: true,
                response: None,
            })?;

        let status = response.status();
        let text = response.text().await.map_err(|e| ChipotleError {
            message: format!("chipotle response read failed: {e}"),
            transient: true,
            response: None,
        })?;
        let parsed = parse_response_body(&text);

        if status.is_success() {
            return Ok(parsed.unwrap_or_else(|| json!({ "raw": text })));
        }

        Err(ChipotleError {
            message: format!("chipotle returned HTTP {status}"),
            transient: is_transient_status(status),
            response: parsed.or_else(|| Some(json!({ "raw": text }))),
        })
    }
}

fn parse_response_body(text: &str) -> Option<Value> {
    if text.trim().is_empty() {
        return Some(json!({}));
    }
    serde_json::from_str::<Value>(text).ok()
}

fn is_transient_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_action_url_trims_base_url() {
        let client = ChipotleClient::new("https://api.example.test/".to_string());
        assert_eq!(
            client.lit_action_url(),
            "https://api.example.test/lit_action"
        );
    }

    #[test]
    fn request_body_uses_code_and_js_params() {
        let req = ChipotleClient::build_lit_action_request(
            "console.log(params.event)".to_string(),
            json!({ "event": { "hello": "world" } }),
        );
        assert_eq!(
            serde_json::to_value(req).unwrap(),
            json!({
                "code": "console.log(params.event)",
                "js_params": { "event": { "hello": "world" } }
            })
        );
    }

    #[test]
    fn transient_statuses_are_limited_to_retryable_classes() {
        assert!(is_transient_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_transient_status(StatusCode::BAD_GATEWAY));
        assert!(!is_transient_status(StatusCode::BAD_REQUEST));
        assert!(!is_transient_status(StatusCode::UNAUTHORIZED));
    }
}
