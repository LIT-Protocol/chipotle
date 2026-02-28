use serde::{Deserialize, Serialize};
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const DSTACK_SOCKET_DEFAULT: &str = "/var/run/dstack.sock";

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

/// Returns the resolved socket path.
///
/// - **Production profile** (`cargo build --profile production`): always
///   `/var/run/dstack.sock` (env override disabled).
/// - **Dev/release profiles**: uses `DSTACK_SOCKET` env var if set, otherwise
///   defaults to `/var/run/dstack.sock`.
fn resolve_socket_path() -> String {
    #[cfg(is_production)]
    {
        DSTACK_SOCKET_DEFAULT.to_string()
    }
    #[cfg(not(is_production))]
    {
        std::env::var("DSTACK_SOCKET").unwrap_or_else(|_| DSTACK_SOCKET_DEFAULT.to_string())
    }
}

/// Check if the dstack socket is available (exists and is a Unix socket).
#[cfg_attr(not(test), allow(dead_code))]
fn socket_available(path: &str) -> bool {
    Path::new(path)
        .metadata()
        .map(|m| m.file_type().is_socket())
        .unwrap_or(false)
}

/// Fetch a TDX quote from the dstack agent.
///
/// Connects via the dstack Unix socket. The path is taken from `DSTACK_SOCKET`
/// (default: `/var/run/dstack.sock`).
///
/// `report_data` is optional hex-encoded data to include in the quote.
/// When `None`, an empty `"0x"` value is sent.
///
/// Returns an error string if the endpoint is unavailable (e.g. not running
/// inside a dstack-enabled TEE and no simulator running).
pub async fn get_quote(report_data: Option<&str>) -> Result<GetQuoteResponse, String> {
    let socket_path = resolve_socket_path();

    if !Path::new(&socket_path).exists() {
        let hint = if socket_path == DSTACK_SOCKET_DEFAULT {
            " — not running inside a dstack-enabled TEE; is the simulator running?"
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

    let mut stream = UnixStream::connect(&socket_path)
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
        // Skip when the socket is available (dstack-enabled TEE or simulator running).
        let path = resolve_socket_path();
        if socket_available(&path) {
            return;
        }
        let result = get_quote(None).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("dstack-enabled TEE") || err.contains("simulator"),
            "unexpected error: {err}"
        );
    }

    /// Validates that the dstack simulator returns a well-formed attestation response.
    ///
    /// Only compiled with `--features phala`. Fails if the dstack socket is not available
    /// (must be a dstack-enabled TEE or simulator running).
    #[cfg(phala)]
    #[tokio::test]
    async fn simulator_returns_valid_attestation() {
        let path = resolve_socket_path();
        assert!(
            socket_available(&path),
            "dstack socket at {} is not available — must be a dstack-enabled TEE or simulator running",
            path
        );

        let resp = get_quote(None)
            .await
            .expect("get_quote() failed against simulator");

        assert!(!resp.quote.is_empty(), "quote must not be empty");
        assert!(!resp.event_log.is_empty(), "event_log must not be empty");
        assert!(!resp.vm_config.is_empty(), "vm_config must not be empty");
    }

    /// Validates that a custom report_data value round-trips through the simulator.
    #[cfg(phala)]
    #[tokio::test]
    async fn simulator_accepts_report_data() {
        let path = resolve_socket_path();
        assert!(
            socket_available(&path),
            "dstack socket at {} is not available — must be a dstack-enabled TEE or simulator running",
            path
        );

        let resp = get_quote(Some("0xdeadbeef"))
            .await
            .expect("get_quote() with report_data failed against simulator");

        // The quote should still be non-empty even with custom report_data.
        assert!(!resp.quote.is_empty(), "quote must not be empty");
    }
}
