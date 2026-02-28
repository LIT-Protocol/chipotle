//! Dstack attestation: fetch TDX quotes from the dstack agent.
//!
//! ## Simulator vs real TDX
//!
//! The dstack simulator does **not** return a real Intel TDX DCAP quote. It uses a
//! pre-built attestation bundle and a different format (e.g. hex encoding, non-standard
//! layout). Real TDX hardware returns a proper DCAP quote that parses with dcap-qvl
//! and tdx-quote.
//!
//! Because of this, the tests and quote parsing logic **switch by profile**:
//! - **Production** (`--profile production`): only accept real TDX quotes (base64,
//!   parseable by dcap-qvl/tdx-quote, zero report_data when None).
//! - **Dev/release**: accept simulator-style quotes (hex decode, pattern-scan fallback,
//!   relaxed report_data checks).

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

/// Decode quote string to bytes.
///
/// - **Production**: base64 only. Real TDX/gateway uses base64 for binary-in-text transport
///   (see [Intel TDX DCAP Quoting Library API](https://download.01.org/intel-sgx/latest/dcap-latest/linux/docs/Intel_TDX_DCAP_Quoting_Library_API.pdf) Appendix 3; quote is raw binary; base64 is standard for JSON/HTTP).
/// - **Dev/release**: try hex first (simulator returns hex), then base64.
/// TODO: VERIFY THIS IS CORRECT and production doesnt also return hex
#[cfg(test)]
fn decode_quote(quote_str: &str) -> Vec<u8> {
    let s = quote_str.trim();
    #[cfg(not(is_production))]
    {
        let hex_str = s.strip_prefix("0x").unwrap_or(s);
        if hex_str.len() > 200 && hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Ok(decoded) = hex::decode(hex_str) {
                return decoded;
            }
        }
    }
    base64_light::base64_decode(s)
}

/// Extract report_data (64 bytes) from a TDX quote.
///
/// - **Production**: only tdx-quote and dcap-qvl (real TDX quotes parse correctly).
/// - **Dev/release**: also scan for 64-byte window matching expected pattern (simulator fallback).
#[cfg(test)]
fn extract_report_data(quote_bytes: &[u8], expected_prefix: Option<&[u8]>) -> Option<[u8; 64]> {
    match tdx_quote::Quote::from_bytes(quote_bytes) {
        Ok(parsed) => return Some(parsed.report_input_data()),
        Err(e) => eprintln!("warn: tdx_quote::Quote::from_bytes failed: {e}"),
    }
    match dcap_qvl::quote::Quote::parse(quote_bytes) {
        Ok(parsed) => {
            return Some(match &parsed.report {
                dcap_qvl::quote::Report::TD10(r) => r.report_data,
                dcap_qvl::quote::Report::TD15(r) => r.base.report_data,
                dcap_qvl::quote::Report::SgxEnclave(_) => return None,
            });
        }
        Err(e) => eprintln!("warn: dcap_qvl::quote::Quote::parse failed: {e}"),
    }
    // If neither parser succeeded, print a hex dump for inspection (help debug invalid TDX).
    eprintln!(
        "warn: quote is not valid TDX; raw quote (hex): {}",
        hex::encode(quote_bytes)
    );

    #[cfg(not(is_production))]
    {
        for i in 0..quote_bytes.len().saturating_sub(64) {
            let window = &quote_bytes[i..i + 64];
            let matches = match expected_prefix {
                None => window.iter().all(|&b| b == 0),
                Some(prefix) => window.starts_with(prefix),
            };
            if matches {
                let mut out = [0u8; 64];
                out.copy_from_slice(window);
                return Some(out);
            }
        }
    }
    None
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

    /// Fails if the dstack socket is unavailable (requires TEE or simulator).
    #[cfg(phala)]
    #[tokio::test]
    async fn fails_when_socket_unavailable() {
        let path = resolve_socket_path();
        assert!(
            socket_available(&path),
            "dstack socket at {} is not available — must be a dstack-enabled TEE or simulator running",
            path
        );
        let _ = get_quote(None).await.expect("get_quote() failed");
    }

    /// Fails if the socket is available but the returned quote is invalid.
    #[cfg(phala)]
    #[tokio::test]
    async fn fails_when_quote_invalid() {
        let path = resolve_socket_path();
        assert!(
            socket_available(&path),
            "dstack socket at {} is not available — must be a dstack-enabled TEE or simulator running",
            path
        );
        let resp = get_quote(None).await.expect("get_quote() failed");
        assert!(!resp.quote.is_empty(), "quote must not be empty");
        assert!(!resp.event_log.is_empty(), "event_log must not be empty");
        assert!(!resp.vm_config.is_empty(), "vm_config must not be empty");

        let quote_bytes = decode_quote(&resp.quote);
        assert!(
            quote_bytes.len() > 100,
            "quote must be substantial (>100 bytes)"
        );

        #[cfg(is_production)]
        {
            let report_data_none = extract_report_data(&quote_bytes, None).expect(
                "quote from get_quote(None) must parse as valid TDX quote (tdx-quote or dcap-qvl)",
            );
            assert!(
                report_data_none.iter().all(|&b| b == 0),
                "report_data must be all zeros when None; got {:02x?}",
                &report_data_none[..8]
            );
        }

        // Validate report_data round-trip: parse quote and verify report_data is embedded
        let resp = get_quote(Some("0xdeadbeef"))
            .await
            .expect("get_quote() with report_data failed");
        assert!(
            !resp.quote.is_empty(),
            "quote with report_data must not be empty"
        );

        let quote_bytes = decode_quote(&resp.quote);
        let expected: [u8; 4] = [0xde, 0xad, 0xbe, 0xef];
        let report_data = extract_report_data(&quote_bytes, Some(&expected))
            .expect("quote must contain 0xdeadbeef in report_data");
        assert!(
            report_data.starts_with(&expected),
            "report_data must contain embedded 0xdeadbeef; got {:02x?}",
            &report_data[..8]
        );
    }
}
