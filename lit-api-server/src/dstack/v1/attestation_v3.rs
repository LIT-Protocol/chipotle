//! Opt-in, platform-independent evidence transport. This endpoint does not verify
//! evidence or claim that the HTTPS connection terminates in the attested CVM.
//! Existing quote endpoints and all key derivation stay on their original paths.

use rocket::{get, http::Status, serde::json::Json};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::time::Duration;
use tokio::sync::Semaphore;

use super::{attestation_v2::parse_nonce, dstack};

const REPORT_DOMAIN: &[u8] = b"chipotle-attestation-v3:";
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
// Fresh hardware attestations are more expensive than v2's cached quote. Do not
// let concurrent public requests build an unbounded queue at the guest agent.
static IN_FLIGHT: Semaphore = Semaphore::const_new(1);

#[derive(Debug, Serialize)]
pub struct AttestationV3Response {
    pub version: u32,
    pub nonce: String,
    /// keccak256(REPORT_DOMAIN || nonce bytes), right-padded to 64 bytes, hex.
    /// This echo is not proof: verifiers must compare against verified evidence.
    pub report_data: String,
    /// Opaque, hex-encoded dstack VersionedAttestation (not a raw Intel quote).
    pub attestation: String,
    /// AMD endorsement certificates for an SEV-SNP report, when this instance
    /// has them cached. Untrusted by construction: verifiers check them
    /// against AMD root keys they pin. Absent on other platforms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collateral: Option<super::amd_collateral::AmdCollateral>,
}

#[derive(Deserialize)]
struct AgentResponse {
    attestation: String,
}

#[derive(rocket::Responder)]
pub struct AttestationV3Reply {
    body: Json<AttestationV3Response>,
    cache_control: rocket::http::Header<'static>,
}

#[derive(Debug, rocket::Responder)]
pub struct AttestationV3Error {
    body: rocket::response::status::Custom<String>,
    cache_control: rocket::http::Header<'static>,
}

fn error(status: Status, message: &str) -> AttestationV3Error {
    AttestationV3Error {
        body: rocket::response::status::Custom(status, message.to_string()),
        cache_control: rocket::http::Header::new("Cache-Control", "no-store"),
    }
}

fn report_data(nonce: &[u8; 32]) -> [u8; 64] {
    let mut digest = Keccak256::new();
    digest.update(REPORT_DOMAIN);
    digest.update(nonce);
    let mut data = [0; 64];
    data[..32].copy_from_slice(&digest.finalize());
    data
}

/// Use the existing agent's platform-adaptive /Attest RPC explicitly. The
/// SDK's attest() does not bound response size or request duration. A separate,
/// socket-only transport enforces both without changing any existing GetKey call.
async fn fetch_attestation(socket: &str, data: &[u8; 64]) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .unix_socket(socket)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("failed to create attestation client: {e}"))?;
    let mut response = client
        .post("http://localhost/Attest")
        .json(&serde_json::json!({ "report_data": hex::encode(data) }))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("dstack Attest unavailable: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("dstack Attest returned {}", response.status()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("failed to read dstack attestation: {e}"))?
    {
        if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(body.len()) {
            return Err("dstack attestation response exceeds size limit".into());
        }
        body.extend_from_slice(&chunk);
    }
    let response: AgentResponse = serde_json::from_slice(&body)
        .map_err(|_| "dstack returned a malformed attestation response".to_string())?;
    let evidence = &response.attestation;
    if evidence.is_empty()
        || !evidence.len().is_multiple_of(2)
        || !evidence.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("dstack returned empty or non-hex attestation evidence".into());
    }
    Ok(response.attestation)
}

/// A new hardware report per request, with no cache or legacy fallback. Clients
/// must verify the entire dstack envelope and its report-data binding themselves.
#[get("/attestation/v3?<nonce>")]
pub async fn attestation_v3(nonce: Option<&str>) -> Result<AttestationV3Reply, AttestationV3Error> {
    let nonce = nonce.unwrap_or_default();
    let parsed = parse_nonce(nonce).ok_or_else(|| {
        error(
            Status::BadRequest,
            "nonce must be 32 bytes as 64 lowercase hex characters",
        )
    })?;
    let _permit = IN_FLIGHT.try_acquire().map_err(|_| {
        error(
            Status::TooManyRequests,
            "attestation generation is busy; retry later",
        )
    })?;
    let data = report_data(&parsed);
    let attestation = fetch_attestation(&dstack::resolve_socket_path(), &data)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "v3 attestation generation failed");
            error(
                Status::ServiceUnavailable,
                "attestation evidence is unavailable",
            )
        })?;
    // Already validated as hex by fetch_attestation.
    let collateral = hex::decode(&attestation)
        .ok()
        .and_then(|evidence| super::amd_collateral::for_evidence(&evidence));
    Ok(AttestationV3Reply {
        cache_control: rocket::http::Header::new("Cache-Control", "no-store"),
        body: Json(AttestationV3Response {
            version: 3,
            nonce: nonce.to_string(),
            report_data: hex::encode(data),
            attestation,
            collateral,
        }),
    })
}

/// Fetches this instance's AMD endorsement certificates soon after start, so
/// the first client is not the one that finds the cache empty. Does nothing on
/// platforms other than SEV-SNP or without a dstack agent. Gives up after a
/// while: requests keep retrying in the background on their own.
pub async fn prewarm_collateral() {
    let socket = dstack::resolve_socket_path();
    if !dstack::socket_available(&socket) {
        return;
    }
    for _ in 0..30 {
        let evidence = {
            let Ok(_permit) = IN_FLIGHT.acquire().await else {
                return;
            };
            fetch_attestation(&socket, &[0u8; 64]).await
        };
        match evidence.map(hex::decode) {
            Ok(Ok(evidence)) => {
                if super::amd_collateral::snp_report(&evidence).is_none()
                    || super::amd_collateral::for_evidence(&evidence).is_some()
                {
                    return;
                }
            }
            Ok(Err(_)) => return,
            Err(e) => tracing::debug!(error = %e, "collateral prewarm could not get evidence"),
        }
        tokio::time::sleep(Duration::from_secs(65)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn challenge_is_domain_separated_and_zero_padded() {
        let nonce = [0x11; 32];
        let data = report_data(&nonce);
        // Cross-tool vector: cast keccak over domain bytes || 0x11 * 32,
        // as used by the deployment verification workflow.
        assert_eq!(
            hex::encode(&data[..32]),
            "c261434695b165c2ad0b967e1642f4a15def329106a93d1ba9c6e4e43ad7db7f"
        );
        let expected = Keccak256::digest([REPORT_DOMAIN, &nonce].concat());
        assert_eq!(&data[..32], expected.as_slice());
        assert_eq!(data[32..], [0; 32]);
        assert_ne!(data, report_data(&[0x12; 32]));
        assert_ne!(&data[..32], Keccak256::digest(nonce).as_slice());
    }

    async fn mock_agent(status: &str, body: &str) -> (Result<String, String>, String) {
        let path = std::env::temp_dir().join(format!("attest-{}.sock", uuid::Uuid::new_v4()));
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let reply = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let agent = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 1024];
            loop {
                let n = socket.read(&mut chunk).await.unwrap();
                assert_ne!(n, 0);
                request.extend_from_slice(&chunk[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length: ")
                                .map(|n| n.parse().unwrap())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            // Oversized-response tests may close the connection before all bytes
            // are written. The client error is what those tests assert.
            let _ = socket.write_all(reply.as_bytes()).await;
            String::from_utf8(request).unwrap()
        });
        let result = fetch_attestation(path.to_str().unwrap(), &report_data(&[0x11; 32])).await;
        let request = agent.await.unwrap();
        std::fs::remove_file(path).unwrap();
        (result, request)
    }

    #[tokio::test]
    async fn forwards_challenge_to_attest_without_touching_keys_or_quote_api() {
        // Synthetic transport bytes, deliberately not a cryptographic fixture.
        for evidence in ["00030102", "83a776657273696f6e01"] {
            let (result, request) =
                mock_agent("200 OK", &format!(r#"{{"attestation":"{evidence}"}}"#)).await;
            assert_eq!(result.unwrap(), evidence);
            assert!(request.starts_with("POST /Attest HTTP/1.1\r\n"));
            let body: serde_json::Value =
                serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
            assert_eq!(
                body,
                serde_json::json!({"report_data": hex::encode(report_data(&[0x11; 32]))})
            );
        }
    }

    #[tokio::test]
    async fn fails_closed_on_unavailable_or_malformed_evidence() {
        for body in [
            r#"{}"#,
            r#"{"attestation":""}"#,
            r#"{"attestation":"xyz"}"#,
            r#"{"attestation":"abc"}"#,
            "not json",
        ] {
            assert!(mock_agent("200 OK", body).await.0.is_err());
        }
        for status in [
            "400 Bad Request",
            "404 Not Found",
            "503 Service Unavailable",
            "302 Found",
        ] {
            assert!(
                mock_agent(status, r#"{"attestation":"00030102"}"#)
                    .await
                    .0
                    .is_err()
            );
        }
        let oversized = format!(
            r#"{{"attestation":"{}"}}"#,
            "ab".repeat(MAX_RESPONSE_BYTES / 2)
        );
        assert!(
            mock_agent("200 OK", &oversized)
                .await
                .0
                .unwrap_err()
                .contains("size limit")
        );
    }

    #[rocket::async_test]
    async fn mounts_alongside_legacy_routes_and_rejects_invalid_nonces() {
        let routes = super::super::endpoints::attestation_routes();
        let paths: Vec<_> = routes.iter().map(|r| r.uri.path().to_string()).collect();
        assert_eq!(
            paths,
            [
                "/attestation",
                "/info",
                "/attestation/v2",
                "/attestation/v3"
            ]
        );
        let client = rocket::local::asynchronous::Client::tracked(
            rocket::build()
                .mount("/", routes)
                .mount("/dstack/v1", super::super::endpoints::routes()),
        )
        .await
        .unwrap();
        for path in ["/attestation/unified", "/dstack/v1/attestation/unified"] {
            assert_eq!(client.get(path).dispatch().await.status(), Status::NotFound);
        }
        for query in [
            "".to_string(),
            "?nonce=00".to_string(),
            format!("?nonce={}", "FF".repeat(32)),
            format!("?nonce={}", "00".repeat(33)),
        ] {
            let response = client
                .get(format!("/attestation/v3{query}"))
                .dispatch()
                .await;
            assert_eq!(response.status(), Status::BadRequest);
            assert_eq!(
                response.headers().get_one("Cache-Control"),
                Some("no-store")
            );
        }
        let _permit = IN_FLIGHT.acquire().await.unwrap();
        let response = client
            .get(format!("/attestation/v3?nonce={}", "00".repeat(32)))
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::TooManyRequests);
        assert_eq!(
            response.headers().get_one("Cache-Control"),
            Some("no-store")
        );
    }

    /// CI explicitly runs this against its real dstack guest-agent simulator.
    /// Ignored locally instead of silently passing when no agent is available.
    #[tokio::test]
    #[ignore = "requires a running dstack agent; CI runs with --ignored"]
    async fn configured_agent_serves_platform_attestation() {
        let socket = dstack::resolve_socket_path();
        assert!(
            dstack::socket_available(&socket),
            "dstack socket is required"
        );
        let evidence = fetch_attestation(&socket, &report_data(&[0x42; 32]))
            .await
            .expect("real agent Attest RPC");
        assert!(!hex::decode(evidence).unwrap().is_empty());
    }
}
