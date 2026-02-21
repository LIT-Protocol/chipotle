use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const DSTACK_SOCKET: &str = "/var/run/dstack.sock";

#[derive(Debug, Serialize)]
struct GetQuoteRequest {
    #[serde(rename = "reportData")]
    report_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetQuoteResponse {
    pub quote: String,
    pub event_log: String,
    pub vm_config: String,
}

/// Fetch a TDX quote from the dstack agent via its Unix socket.
///
/// `report_data` is optional hex-encoded data to include in the quote.
/// When `None`, an empty `"0x"` value is sent.
///
/// Returns an error string if the socket is unavailable (i.e. not running
/// inside a Phala CVM) or if the request fails for any other reason.
pub async fn get_quote(report_data: Option<&str>) -> Result<GetQuoteResponse, String> {
    if !Path::new(DSTACK_SOCKET).exists() {
        return Err(format!(
            "dstack socket not found at {DSTACK_SOCKET} — not running inside a Phala CVM"
        ));
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

    let mut stream = UnixStream::connect(DSTACK_SOCKET)
        .await
        .map_err(|e| format!("failed to connect to dstack socket: {e}"))?;

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
        assert_eq!(json, r#"{"reportData":"0xdeadbeef"}"#);
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
        // Unless running inside a CVM, the socket won't exist.
        let result = get_quote(None).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("not running inside a Phala CVM"),
            "unexpected error: {err}"
        );
    }
}
