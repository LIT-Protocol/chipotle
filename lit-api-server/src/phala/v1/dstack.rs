use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const DSTACK_SOCKET: &str = "/var/run/dstack.sock";

#[derive(Debug, Serialize)]
struct GetQuoteRequest {
    report_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetQuoteResponse {
    pub quote: String,
    pub event_log: String,
    pub vm_config: String,
}

enum Endpoint {
    Http(String),
    Socket(String),
}

/// Resolve which dstack endpoint to use.
///
/// Priority:
/// 1. `DSTACK_SIMULATOR_ENDPOINT` env var — if set, parsed as:
///    - HTTP mode  if it starts with `http://` or `https://`
///    - Unix socket mode otherwise (value is treated as a socket path)
/// 2. Default socket at `/var/run/dstack.sock`
fn resolve_endpoint() -> Endpoint {
    if let Ok(env) = std::env::var("DSTACK_SIMULATOR_ENDPOINT") {
        if env.starts_with("http://") || env.starts_with("https://") {
            return Endpoint::Http(env);
        } else {
            return Endpoint::Socket(env);
        }
    }
    Endpoint::Socket(DSTACK_SOCKET.to_string())
}

/// Fetch a TDX quote from the dstack agent.
///
/// Connects via the dstack Unix socket (`/var/run/dstack.sock`) unless
/// `DSTACK_SIMULATOR_ENDPOINT` is set, in which case the simulator is used
/// (socket path or HTTP URL).
///
/// `report_data` is optional hex-encoded data to include in the quote.
/// When `None`, an empty `"0x"` value is sent.
///
/// Returns an error string if the endpoint is unavailable (e.g. not running
/// inside a Phala CVM and no simulator configured).
pub async fn get_quote(report_data: Option<&str>) -> Result<GetQuoteResponse, String> {
    match resolve_endpoint() {
        Endpoint::Http(url) => get_quote_http(&url, report_data).await,
        Endpoint::Socket(path) => get_quote_socket(&path, report_data).await,
    }
}

async fn get_quote_http(
    base_url: &str,
    report_data: Option<&str>,
) -> Result<GetQuoteResponse, String> {
    let body = serde_json::to_string(&GetQuoteRequest {
        report_data: report_data.unwrap_or("0x").to_string(),
    })
    .map_err(|e| format!("failed to serialize request: {e}"))?;

    let url = format!("{}/GetQuote", base_url.trim_end_matches('/'));
    let client = Client::new();
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("failed to reach dstack simulator at {url}: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "dstack simulator at {url} returned HTTP {}",
            response.status()
        ));
    }

    response
        .json::<GetQuoteResponse>()
        .await
        .map_err(|e| format!("failed to parse simulator response: {e}"))
}

async fn get_quote_socket(
    socket_path: &str,
    report_data: Option<&str>,
) -> Result<GetQuoteResponse, String> {
    if !Path::new(socket_path).exists() {
        let hint = if socket_path == DSTACK_SOCKET {
            " — not running inside a Phala CVM"
        } else {
            " — simulator socket not found; is the simulator running?"
        };
        return Err(format!("dstack socket not found at {socket_path}{hint}"));
    }

    let body = serde_json::to_string(&GetQuoteRequest {
        report_data: report_data.unwrap_or("0x").to_string(),
    })
    .map_err(|e| format!("failed to serialize request: {e}"))?;

    let request = format!(
        "POST /GetQuote HTTP/1.1\r\n\
         Host: localhost\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        body.len(),
        body,
    );

    let mut stream = UnixStream::connect(socket_path)
        .await
        .map_err(|e| format!("failed to connect to dstack socket at {socket_path}: {e}"))?;

    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("failed to write to dstack socket: {e}"))?;

    stream
        .shutdown()
        .await
        .map_err(|e| format!("failed to shutdown write half: {e}"))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|e| format!("failed to read dstack response: {e}"))?;

    let response_str =
        String::from_utf8(response).map_err(|e| format!("invalid UTF-8 in response: {e}"))?;

    // Split HTTP headers from body at the blank line.
    let body_str = response_str
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or(&response_str);

    serde_json::from_str::<GetQuoteResponse>(body_str)
        .map_err(|e| format!("failed to parse dstack response: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_request() {
        let req = GetQuoteRequest {
            report_data: "0xdeadbeef".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"report_data":"0xdeadbeef"}"#);
    }

    #[test]
    fn deserialize_response() {
        let json = r#"{
            "quote": "base64encodedquote==",
            "event_log": "some event log",
            "vm_config": "some vm config"
        }"#;
        let resp: GetQuoteResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.quote, "base64encodedquote==");
        assert_eq!(resp.event_log, "some event log");
        assert_eq!(resp.vm_config, "some vm config");
    }

    #[tokio::test]
    async fn missing_socket_returns_descriptive_error() {
        // Skip this test when a simulator is configured — the endpoint is real.
        if std::env::var("DSTACK_SIMULATOR_ENDPOINT").is_ok() {
            return;
        }
        // Unless running inside a CVM, the default socket won't exist.
        let result = get_quote(None).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("not running inside a Phala CVM"),
            "unexpected error: {err}"
        );
    }

    /// Validates that the dstack simulator returns a well-formed attestation response.
    ///
    /// This test is a no-op unless `DSTACK_SIMULATOR_ENDPOINT` is set, so it
    /// is safe to run in unit-test suites. The phala-simulator CI job sets the
    /// variable and runs this test explicitly to gate the PR.
    #[tokio::test]
    async fn simulator_returns_valid_attestation() {
        if std::env::var("DSTACK_SIMULATOR_ENDPOINT").is_err() {
            // Simulator not configured — skip.
            return;
        }

        let resp = get_quote(None)
            .await
            .expect("get_quote() failed against simulator");

        assert!(!resp.quote.is_empty(), "quote must not be empty");
        assert!(!resp.event_log.is_empty(), "event_log must not be empty");
        assert!(!resp.vm_config.is_empty(), "vm_config must not be empty");
    }

    /// Validates that a custom report_data value round-trips through the simulator.
    #[tokio::test]
    async fn simulator_accepts_report_data() {
        if std::env::var("DSTACK_SIMULATOR_ENDPOINT").is_err() {
            return;
        }

        let resp = get_quote(Some("0xdeadbeef"))
            .await
            .expect("get_quote() with report_data failed against simulator");

        // The quote should still be non-empty even with custom report_data.
        assert!(!resp.quote.is_empty(), "quote must not be empty");
    }
}
