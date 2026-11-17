//! Dstack attestation: fetch TDX quotes from the dstack agent.
//!
//! ## Simulator vs real TDX
//!
//! The dstack simulator does **not** return a real Intel TDX DCAP quote. It uses a
//! pre-built attestation bundle and a different format (e.g. hex encoding, non-standard
//! layout). Real TDX hardware returns a proper DCAP quote that parses with dcap-qvl
//! and tdx-quote.
//!
//! Because of this, the tests and quote parsing logic **switch by features**:
//! - **Production** (`--features production`): only accept real TDX quotes (base64,
//!   parseable by dcap-qvl/tdx-quote, zero report_data when None).
//! - **Dev** (`--features dstack`): accept simulator-style quotes (hex decode, pattern-scan fallback,
//!   relaxed report_data checks).

use dstack_sdk::dstack_client;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
const DSTACK_SOCKET_DEFAULT: &str = "/var/run/dstack.sock";

/// Returns the resolved socket path.
///
/// - **Production** (`--features production`): always
///   `/var/run/dstack.sock` (env override disabled).
/// - **Dev** (`--features dstack`): uses `DSTACK_SOCKET` env var if set, otherwise
///   defaults to `/var/run/dstack.sock`.
pub(super) fn resolve_socket_path() -> String {
    #[cfg(feature = "production")]
    {
        DSTACK_SOCKET_DEFAULT.to_string()
    }
    #[cfg(not(feature = "production"))]
    {
        std::env::var("DSTACK_SOCKET").unwrap_or_else(|_| DSTACK_SOCKET_DEFAULT.to_string())
    }
}

/// Get the socket path from the environment or default to `/var/run/dstack.sock`.
fn get_socket_path() -> Result<String, String> {
    let socket_path = resolve_socket_path();
    if !Path::new(&socket_path).exists() {
        let hint = if socket_path == DSTACK_SOCKET_DEFAULT {
            " — not running inside a dstack-enabled TEE; is the simulator running?"
        } else {
            " — simulator socket not found; is the simulator running?"
        };
        return Err(format!("dstack socket not found at {socket_path}{hint}"));
    }

    Ok(socket_path)
}

/// Check if the dstack socket is available (exists and is a Unix socket).
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn socket_available(path: &str) -> bool {
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
pub async fn get_quote(
    report_data: Option<&str>,
) -> Result<dstack_client::GetQuoteResponse, String> {
    let socket_path = get_socket_path()?;
    let endpoint = Some(socket_path.as_str());
    let client = dstack_client::DstackClient::new(endpoint);
    // The dstack SDK takes raw bytes and hex-encodes them internally before sending
    // to the dstack socket. Callers pass a hex string (e.g. "0xdeadbeef"), so
    // decode it to actual bytes first. None/empty means all-zero report_data.
    let report_data_bytes: Vec<u8> = match report_data {
        None | Some("0x") | Some("") => vec![0u8; 64],
        Some(s) => {
            let hex_str = s.strip_prefix("0x").unwrap_or(s);
            hex::decode(hex_str).map_err(|e| format!("invalid hex in report_data '{s}': {e}"))?
        }
    };
    let quote = client
        .get_quote(report_data_bytes)
        .await
        .map_err(|e| format!("failed to get quote: {e}"))?;
    Ok(quote)
}

/// Fetch a platform-neutral attestation bundle (Intel TDX or AMD SEV-SNP).
/// Uses the legacy key API unchanged; this does not migrate key derivation.
pub async fn attest() -> Result<dstack_client::AttestResponse, String> {
    attest_at(&get_socket_path()?).await
}

async fn attest_at(socket_path: &str) -> Result<dstack_client::AttestResponse, String> {
    dstack_client::DstackClient::new(Some(socket_path))
        .attest(vec![0u8; 64])
        .await
        .map_err(|e| format!("failed to attest: {e}"))
}

/// Preserve platform-specific Info fields, including null TDX measurements on AMD.
/// The SDK's typed InfoResponse still requires TDX measurement strings.
pub async fn get_info_document() -> Result<serde_json::Value, String> {
    info_document_at(&get_socket_path()?).await
}

async fn info_document_at(socket_path: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .unix_socket(socket_path.to_owned())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("failed to create info client: {e}"))?;
    let mut document: serde_json::Value = client
        .post("http://localhost/Info")
        .json(&serde_json::json!({}))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("failed to get info: {e}"))?
        .json()
        .await
        .map_err(|e| format!("invalid info response: {e}"))?;
    if let Some(tcb) = document.get("tcb_info").and_then(serde_json::Value::as_str) {
        document["tcb_info"] =
            serde_json::from_str(tcb).map_err(|e| format!("invalid TCB info: {e}"))?;
    }
    Ok(document)
}

/// Fetch worker info from the dstack agent.
///
/// Connects via the dstack Unix socket. Returns app_id, instance_id, tcb_info,
/// compose_hash, etc. per [dstack HTTP API](https://github.com/Dstack-TEE/dstack/blob/master/sdk/curl/api.md).
/// Verifiers use `tcb_info.app_compose` and `compose_hash` for compose-hash verification.
#[cfg(test)]
pub async fn get_info() -> Result<dstack_client::InfoResponse, String> {
    let socket_path = get_socket_path()?;
    let endpoint = Some(socket_path.as_str());
    let client = dstack_client::DstackClient::new(endpoint);
    let info = client
        .info()
        .await
        .map_err(|e| format!("failed to get info: {e}"))?;
    Ok(info)
}

pub async fn get_key(path: &str, purpose: &str) -> Result<dstack_client::GetKeyResponse, String> {
    let socket_path = get_socket_path()?;
    let endpoint = Some(socket_path.as_str());
    let client = dstack_client::DstackClient::new(endpoint);
    let path = Some(path.to_string());
    let purpose = Some(purpose.to_string());
    let key_provider_info = client
        .get_key(path, purpose)
        .await
        .map_err(|e| format!("failed to get key provider info: {e}"))?;
    Ok(key_provider_info)
}

/// Decode quote string to bytes.
///
/// - **Production**: base64 only. Real TDX/gateway uses base64 for binary-in-text transport
///   (see [Intel TDX DCAP Quoting Library API](https://download.01.org/intel-sgx/latest/dcap-latest/linux/docs/Intel_TDX_DCAP_Quoting_Library_API.pdf) Appendix 3; quote is raw binary; base64 is standard for JSON/HTTP).
/// - **Dev/release**: try hex first (simulator returns hex), then base64.
///   TODO: VERIFY THIS IS CORRECT and production doesnt also return hex
#[cfg(test)]
fn decode_quote(quote_str: &str) -> Vec<u8> {
    let s = quote_str.trim();
    #[cfg(not(feature = "production"))]
    {
        let hex_str = s.strip_prefix("0x").unwrap_or(s);
        if hex_str.len() > 200
            && hex_str.chars().all(|c| c.is_ascii_hexdigit())
            && let Ok(decoded) = hex::decode(hex_str)
        {
            return decoded;
        }
    }
    base64_light::base64_decode(s)
}

/// Extract report_data (64 bytes) from a TDX quote.
///
/// - **Production**: only tdx-quote and dcap-qvl (real TDX quotes parse correctly).
/// - **Dev/release**: also scan for 64-byte window matching expected pattern (simulator fallback).
#[cfg(test)]
fn extract_report_data(quote_bytes: &[u8], _expected_prefix: Option<&[u8]>) -> Option<[u8; 64]> {
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

    #[cfg(not(feature = "production"))]
    {
        for i in 0..quote_bytes.len().saturating_sub(64) {
            let window = &quote_bytes[i..i + 64];
            let matches = match _expected_prefix {
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
    fn deserialize_response() {
        let json = r#"{
            "quote": "base64encodedquote==",
            "event_log": "some event log",
            "vm_config": "some vm config"
        }"#;
        let resp: dstack_client::GetQuoteResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.quote, "base64encodedquote==");
        assert_eq!(resp.event_log, "some event log");
        assert_eq!(resp.vm_config, "some vm config");
    }

    /// Exercises `get_quote` when a dstack Unix socket is present (TEE or simulator).
    /// No-ops when absent so `cargo test --all-features` passes on typical CI; the
    /// phala-simulator workflow runs these against a real simulator socket.
    #[tokio::test]
    async fn test_get_quote_succeeds_when_socket_available() {
        let path = resolve_socket_path();
        if !socket_available(&path) {
            return;
        }
        let _ = get_quote(None).await.expect("get_quote() failed");
    }

    /// Validates quote shape when the socket is available; otherwise skips (see
    /// `test_get_quote_succeeds_when_socket_available`).
    #[tokio::test]
    async fn fails_when_quote_invalid() {
        let path = resolve_socket_path();
        if !socket_available(&path) {
            return;
        }
        let resp = get_quote(None).await.expect("get_quote() failed");
        assert!(!resp.quote.is_empty(), "quote must not be empty");
        assert!(!resp.event_log.is_empty(), "event_log must not be empty");
        assert!(!resp.vm_config.is_empty(), "vm_config must not be empty");

        let quote_bytes = decode_quote(&resp.quote);
        assert!(
            quote_bytes.len() > 100,
            "quote must be substantial (>100 bytes)"
        );

        #[cfg(feature = "production")]
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

#[cfg(test)]
mod unified_rpc_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_SOCKET: AtomicUsize = AtomicUsize::new(0);

    fn agent(response: serde_json::Value) -> (String, std::thread::JoinHandle<String>) {
        let path = format!(
            "/tmp/chipotle-rpc-{}-{}.sock",
            std::process::id(),
            NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
        );
        let listener = UnixListener::bind(&path).expect("test socket");
        let cleanup = path.clone();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("client connected");
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            loop {
                let n = stream.read(&mut buffer).expect("request bytes");
                assert!(n > 0, "incomplete request");
                request.extend_from_slice(&buffer[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let body = response.to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            std::fs::remove_file(cleanup).unwrap();
            String::from_utf8(request).unwrap()
        });
        (path, worker)
    }

    #[tokio::test]
    async fn unified_attestation_uses_attest_and_preserves_bundle() {
        let (socket, worker) = agent(serde_json::json!({"attestation": "01020304"}));
        let evidence = attest_at(&socket).await.unwrap();
        assert_eq!(evidence.attestation, "01020304");
        let request = worker.join().unwrap();
        assert!(request.starts_with("POST /Attest "));
        let body: serde_json::Value =
            serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["report_data"], "00".repeat(64));
    }

    #[tokio::test]
    async fn info_preserves_null_tdx_fields_and_unknown_platform_fields() {
        let (socket, worker) = agent(serde_json::json!({
            "app_id": "abc", "tcb_info": "{\"mrtd\":null,\"platform\":\"sev-snp\"}"
        }));
        let info = info_document_at(&socket).await.unwrap();
        assert_eq!(info["app_id"], "abc");
        assert!(info["tcb_info"]["mrtd"].is_null());
        assert_eq!(info["tcb_info"]["platform"], "sev-snp");
        assert!(worker.join().unwrap().starts_with("POST /Info "));
    }

    #[tokio::test]
    async fn info_rejects_invalid_nested_json() {
        let (socket, worker) = agent(serde_json::json!({"tcb_info": "not json"}));
        assert!(
            info_document_at(&socket)
                .await
                .unwrap_err()
                .contains("invalid TCB info")
        );
        worker.join().unwrap();
    }
}
