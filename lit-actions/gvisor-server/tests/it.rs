//! End-to-end tests for the any-language runner using the (non-isolating)
//! process sandbox runtime: real gRPC op-loop over a Unix socket, real
//! bundles, the real guest `lit` CLI — only `runsc` itself is substituted,
//! since gVisor requires Linux + the base rootfs image.
//!
//! The test client below plays the role of lit-api-server, exactly like
//! `lit-actions/tests/it.rs` does for the JS runner.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use lit_actions_gvisor_server::bundle::BundleCache;
use lit_actions_gvisor_server::oploop::*;
use lit_actions_gvisor_server::sandbox::ProcessRuntime;
use lit_actions_gvisor_server::supervisor::Supervisor;
use lit_actions_gvisor_server::{start_server, unix};
use pretty_assertions::assert_eq;
use tokio_stream::StreamExt as _;
use tonic::{Code, Request};

const TEST_PKP_SECRET: &str = "0xtest-pkp-secret";
const TEST_ACTION_SECRET: &str = "0xtest-action-secret";

/// Build a base64(tar.gz) bundle from (path, contents) pairs.
fn make_bundle(files: &[(&str, &str)]) -> String {
    use std::io::Write as _;
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        for (name, contents) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, name, contents.as_bytes())
                .unwrap();
        }
        builder.finish().unwrap();
    }
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&tar_bytes).unwrap();
    BASE64.encode(gz.finish().unwrap())
}

/// Bundle with a `run.sh` entrypoint.
fn sh_bundle(script: &str) -> String {
    make_bundle(&[
        ("lit.json", r#"{"entrypoint": ["/bin/sh", "run.sh"]}"#),
        ("run.sh", script),
    ])
}

struct TestServer {
    socket_file: temp_file::TempFile,
    // Owns the bundle cache dir for the server's lifetime.
    _cache_dir: tempfile::TempDir,
}

impl TestServer {
    fn start() -> Self {
        let socket_file = temp_file::empty();
        let socket_path = socket_file.path().to_path_buf();
        let cache_dir = tempfile::tempdir().unwrap();

        // The `lit` guest CLI sits next to the test binary's target dir.
        let guest_bin_dir = PathBuf::from(env!("CARGO_BIN_EXE_lit"))
            .parent()
            .unwrap()
            .to_path_buf();

        let supervisor = Supervisor::new(
            Arc::new(ProcessRuntime { guest_bin_dir }),
            BundleCache::new(cache_dir.path()).unwrap(),
        );

        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .expect("failed to create runtime")
                .block_on(async move {
                    let signal = async {
                        let _ = tokio::signal::ctrl_c().await;
                    };
                    start_server(socket_path, Some(signal), supervisor)
                        .await
                        .expect("failed to start gvisor action server");
                });
        });

        Self {
            socket_file,
            _cache_dir: cache_dir,
        }
    }
}

/// What the fake lit-api-server accumulated over one execution.
#[derive(Debug, Default)]
struct Outcome {
    result: ExecutionResult,
    logs: String,
    response: String,
    fetch_count: u32,
}

/// Plays lit-api-server: pumps the op-loop, answering every op with a
/// canned response (mirrors `Client::execute_js_inner` + `handle_op`).
#[derive(Default)]
struct TestClient {
    /// Reply to UpdateResourceUsage with cancel_action=true.
    cancel_on_tick: bool,
}

impl TestClient {
    async fn execute(
        &self,
        server: &TestServer,
        request: impl Into<ExecutionRequest>,
    ) -> Result<Outcome> {
        let request = request.into();
        let (outbound_tx, outbound_rx) = flume::bounded(0);

        // The server binds its socket asynchronously; retry the first connect.
        let channel = {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                match unix::connect_to_socket(server.socket_file.path()).await {
                    Ok(channel) => break channel,
                    Err(_) if std::time::Instant::now() < deadline => {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                    Err(e) => return Err(e),
                }
            }
        };
        let mut stream = ActionClient::new(channel)
            .execute_js(Request::new(outbound_rx.into_stream()))
            .await?
            .into_inner();

        outbound_tx.send_async(request.into()).await?;

        let mut outcome = Outcome::default();
        while let Some(resp) = stream.try_next().await? {
            match resp.union {
                Some(UnionResponse::Result(res)) => {
                    outcome.result = res;
                    return Ok(outcome);
                }
                Some(op) => {
                    let reply = self.handle_op(op, &mut outcome);
                    outbound_tx.send_async(reply).await?;
                }
                None => {}
            }
        }
        bail!("Server unexpectedly closed connection")
    }

    fn handle_op(&self, op: UnionResponse, outcome: &mut Outcome) -> ExecuteJsRequest {
        match op {
            UnionResponse::Print(req) => {
                outcome.logs.push_str(&req.message);
                PrintResponse {}.into()
            }
            UnionResponse::SetResponse(req) => {
                outcome.response = req.response;
                SetResponseResponse {}.into()
            }
            UnionResponse::UpdateResourceUsage(_) => UpdateResourceUsageResponse {
                cancel_action: self.cancel_on_tick,
            }
            .into(),
            UnionResponse::IncrementFetchCount(_) => {
                outcome.fetch_count += 1;
                IncrementFetchCountResponse {
                    fetch_count: outcome.fetch_count,
                }
                .into()
            }
            UnionResponse::GetPrivateKey(req) => {
                assert!(!req.pkp_id.is_empty());
                GetPrivateKeyResponse {
                    secret: TEST_PKP_SECRET.to_string(),
                }
                .into()
            }
            UnionResponse::GetLitActionPrivateKey(_) => GetLitActionPrivateKeyResponse {
                secret: TEST_ACTION_SECRET.to_string(),
            }
            .into(),
            UnionResponse::GetLitActionPublicKey(req) => GetLitActionPublicKeyResponse {
                public_key: format!("pubkey-of-{}", req.ipfs_id),
            }
            .into(),
            UnionResponse::GetLitActionWalletAddress(req) => GetLitActionWalletAddressResponse {
                wallet_address: format!("wallet-of-{}", req.ipfs_id),
            }
            .into(),
            UnionResponse::AesEncrypt(req) => AesEncryptResponse {
                ciphertext: format!("enc({})", req.message),
            }
            .into(),
            UnionResponse::AesDecrypt(req) => AesDecryptResponse {
                plaintext: format!("dec({})", req.ciphertext),
            }
            .into(),
            UnionResponse::Result(_) => unreachable!("handled in main loop"),
        }
    }
}

fn exec_request(code: String) -> ExecutionRequest {
    ExecutionRequest {
        code,
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_code_returns_success() -> Result<()> {
    let server = TestServer::start();
    let outcome = TestClient::default()
        .execute(&server, exec_request("  \n\t ".to_string()))
        .await?;
    assert!(outcome.result.success);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn sh_bundle_prints_and_sets_response() -> Result<()> {
    let server = TestServer::start();
    let code = sh_bundle(
        r#"
lit print "hello from sh"
lit set-response '{"ok":true}'
"#,
    );
    let outcome = TestClient::default()
        .execute(&server, exec_request(code))
        .await?;
    assert!(outcome.result.success, "error: {}", outcome.result.error);
    assert!(
        outcome.logs.contains("hello from sh\n"),
        "logs: {:?}",
        outcome.logs
    );
    assert_eq!(outcome.response, r#"{"ok":true}"#);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn stdout_is_forwarded_as_logs() -> Result<()> {
    let server = TestServer::start();
    let code = sh_bundle("echo plain stdout works\n");
    let outcome = TestClient::default()
        .execute(&server, exec_request(code))
        .await?;
    assert!(outcome.result.success, "error: {}", outcome.result.error);
    assert!(
        outcome.logs.contains("plain stdout works\n"),
        "logs: {:?}",
        outcome.logs
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn non_utf8_output_keeps_draining_and_completes() -> Result<()> {
    let server = TestServer::start();
    // Invalid UTF-8 on stdout must not stop the log forwarder (an undrained
    // pipe would block the guest); the run still completes normally.
    let code = sh_bundle(
        r#"
printf 'before binary\n'
printf '\377\376\375 raw bytes\n'
printf 'after binary\n'
lit set-response survived
"#,
    );
    let outcome = TestClient::default()
        .execute(&server, exec_request(code))
        .await?;
    assert!(outcome.result.success, "error: {}", outcome.result.error);
    assert!(
        outcome.logs.contains("before binary\n") && outcome.logs.contains("after binary\n"),
        "logs: {:?}",
        outcome.logs
    );
    assert_eq!(outcome.response, "survived");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn secrets_round_trip_through_guest_cli() -> Result<()> {
    let server = TestServer::start();
    let code = sh_bundle(
        r#"
KEY=$(lit get-private-key pkp-123)
ACTION_KEY=$(lit get-action-private-key)
lit set-response "{\"key\":\"$KEY\",\"actionKey\":\"$ACTION_KEY\"}"
"#,
    );
    let outcome = TestClient::default()
        .execute(&server, exec_request(code))
        .await?;
    assert!(outcome.result.success, "error: {}", outcome.result.error);
    assert_eq!(
        outcome.response,
        format!(r#"{{"key":"{TEST_PKP_SECRET}","actionKey":"{TEST_ACTION_SECRET}"}}"#)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn js_params_flow_to_guest() -> Result<()> {
    let server = TestServer::start();
    let code = sh_bundle("lit set-response \"$(lit params)\"\n");
    let request = ExecutionRequest {
        code,
        js_params: Some(br#"{"a":1}"#.to_vec()),
        ..Default::default()
    };
    let outcome = TestClient::default().execute(&server, request).await?;
    assert!(outcome.result.success, "error: {}", outcome.result.error);
    assert_eq!(outcome.response, r#"{"a":1}"#);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn nonzero_exit_reports_failure_with_stderr() -> Result<()> {
    let server = TestServer::start();
    let code = sh_bundle("echo boom >&2\nexit 3\n");
    let outcome = TestClient::default()
        .execute(&server, exec_request(code))
        .await?;
    assert!(!outcome.result.success);
    assert!(
        outcome.result.error.contains("exited with"),
        "error: {}",
        outcome.result.error
    );
    assert!(
        outcome.result.error.contains("boom"),
        "error: {}",
        outcome.result.error
    );
    // stderr is also part of the action logs.
    assert!(outcome.logs.contains("boom\n"), "logs: {:?}", outcome.logs);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn timeout_kills_sandbox_with_deadline_exceeded() -> Result<()> {
    let server = TestServer::start();
    let code = sh_bundle("sleep 30\n");
    let request = ExecutionRequest {
        code,
        timeout: Some(500),
        ..Default::default()
    };
    let err = TestClient::default()
        .execute(&server, request)
        .await
        .unwrap_err();
    let status = err
        .downcast_ref::<tonic::Status>()
        .context("expected tonic::Status")?;
    assert_eq!(status.code(), Code::DeadlineExceeded);
    assert!(
        status.message().contains("maximum runtime of 500ms"),
        "message: {}",
        status.message()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn billing_cancel_terminates_action() -> Result<()> {
    let server = TestServer::start();
    let code = sh_bundle("sleep 30\n");
    let client = TestClient {
        cancel_on_tick: true,
    };
    let err = client
        .execute(&server, exec_request(code))
        .await
        .unwrap_err();
    let status = err
        .downcast_ref::<tonic::Status>()
        .context("expected tonic::Status")?;
    assert_eq!(status.code(), Code::ResourceExhausted);
    assert!(
        status.message().contains("ran out of funds"),
        "message: {}",
        status.message()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn cid_reference_reuses_cached_bundle() -> Result<()> {
    let server = TestServer::start();
    let client = TestClient::default();

    // First run ships the bytes under a server-supplied CID.
    let request = ExecutionRequest {
        code: sh_bundle("lit set-response cached-run\n"),
        ipfs_id: Some("QmCachedBundle1".to_string()),
        ..Default::default()
    };
    let outcome = client.execute(&server, request).await?;
    assert!(outcome.result.success, "error: {}", outcome.result.error);

    // Second run references the cache — no bundle bytes on the wire.
    let request = ExecutionRequest {
        code: "cid:QmCachedBundle1".to_string(),
        ipfs_id: Some("QmCachedBundle1".to_string()),
        ..Default::default()
    };
    let outcome = client.execute(&server, request).await?;
    assert!(outcome.result.success, "error: {}", outcome.result.error);
    assert_eq!(outcome.response, "cached-run");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn uncached_cid_reference_fails_cleanly() -> Result<()> {
    let server = TestServer::start();
    let outcome = TestClient::default()
        .execute(&server, exec_request("cid:QmNeverSeen".to_string()))
        .await?;
    assert!(!outcome.result.success);
    assert!(
        outcome.result.error.contains("not cached"),
        "error: {}",
        outcome.result.error
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_bundle_fails_cleanly() -> Result<()> {
    let server = TestServer::start();
    // Not base64 at all.
    let outcome = TestClient::default()
        .execute(&server, exec_request("!!! not a bundle !!!".to_string()))
        .await?;
    assert!(!outcome.result.success);
    assert!(
        outcome.result.error.contains("base64"),
        "error: {}",
        outcome.result.error
    );

    // Valid base64, but not a tar archive.
    let outcome = TestClient::default()
        .execute(&server, exec_request(BASE64.encode(b"not a tarball")))
        .await?;
    assert!(!outcome.result.success);
    assert!(
        outcome.result.error.contains("tar"),
        "error: {}",
        outcome.result.error
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn aes_and_fetch_count_ops_work() -> Result<()> {
    let server = TestServer::start();
    let code = sh_bundle(
        r#"
CT=$(lit aes-encrypt pkp-1 secret-msg)
PT=$(lit aes-decrypt pkp-1 "$CT")
N=$(lit increment-fetch-count)
lit set-response "$CT|$PT|$N"
"#,
    );
    let outcome = TestClient::default()
        .execute(&server, exec_request(code))
        .await?;
    assert!(outcome.result.success, "error: {}", outcome.result.error);
    assert_eq!(outcome.response, "enc(secret-msg)|dec(enc(secret-msg))|1");
    Ok(())
}
