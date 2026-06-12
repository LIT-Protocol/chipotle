use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Result, bail};
use indoc::{formatdoc, indoc};
use lit_actions_server::proto::execute_js_request::AesEncryptResponse;
use lit_actions_server::worker_pool::PoolHealth;
use lit_actions_server::{TestServer, get_lit_action_ipfs_id, init_v8, proto::*, unix};
use pretty_assertions::assert_eq;
use rstest::*;
use temp_file::TempFile;
use tokio_stream::StreamExt as _;
use tonic::{Code, Request, Status};

// This client is only used for testing. The real implementation is provided by lit-node.
#[derive(Debug)]
struct TestClient {
    socket_file: TempFile,
    messages: gotham_store::GothamStore,
}

impl Drop for TestClient {
    fn drop(&mut self) {
        if !self.messages.is_empty() && !std::thread::panicking() {
            panic!(
                "GothamStore still contains {} type(s) to be inspected via `take`",
                self.messages.len()
            );
        }
    }
}

impl TestClient {
    fn new(socket_file: TempFile) -> Self {
        Self {
            socket_file,
            messages: Default::default(),
        }
    }

    async fn execute_js(
        &mut self,
        request: impl Into<ExecutionRequest>,
    ) -> Result<ExecutionResult> {
        let request = request.into();

        let (outbound_tx, outbound_rx) = flume::bounded(0);
        // TestServer::start() spawns the server asynchronously, so the very
        // first request can race the unix-socket bind and get "Connection
        // refused". Retry the connect briefly instead of failing the test.
        let channel = {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                match unix::connect_to_socket(self.socket_file.path()).await {
                    Ok(channel) => break channel,
                    Err(_) if std::time::Instant::now() < deadline => {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                    Err(e) => return Err(e),
                }
            }
        };
        let mut client = ActionClient::new(channel);

        let response = client
            .execute_js(Request::new(outbound_rx.into_stream()))
            .await?;
        let mut stream = response.into_inner();

        // Send initial execution request to server
        outbound_tx.send_async(request.into()).await?;

        // Handle responses from server
        while let Some(resp) = stream.try_next().await? {
            match resp.union {
                // Return final result from server
                Some(UnionResponse::Result(res)) => {
                    self.messages.put(res.clone());
                    if !res.success {
                        bail!(res.error);
                    }
                    return Ok(res);
                }
                // Handle op requests
                Some(op) => {
                    let resp = if let Some(resp) = self.messages.try_take::<ErrorResponse>() {
                        resp.into()
                    } else {
                        self.handle_op(op)
                    };
                    outbound_tx.send_async(resp).await?;
                }
                // Ignore empty responses
                None => {}
            };
        }

        bail!("Server unexpectedly closed connection")
    }

    fn handle_op(&mut self, op: UnionResponse) -> ExecuteJsRequest {
        match op {
            UnionResponse::GetPrivateKey(req) => {
                self.messages.put(req);
                self.messages.take::<GetPrivateKeyResponse>().into()
            }
            UnionResponse::GetLitActionPrivateKey(req) => {
                self.messages.put(req);
                self.messages
                    .take::<GetLitActionPrivateKeyResponse>()
                    .into()
            }
            UnionResponse::GetLitActionPublicKey(req) => {
                self.messages.put(req);
                self.messages.take::<GetLitActionPublicKeyResponse>().into()
            }
            UnionResponse::GetLitActionWalletAddress(req) => {
                self.messages.put(req);
                self.messages
                    .take::<GetLitActionWalletAddressResponse>()
                    .into()
            }
            UnionResponse::SetResponse(req) => {
                self.messages.put(req);
                self.messages.take::<SetResponseResponse>().into()
            }
            UnionResponse::Print(req) => {
                self.messages.put(req);
                self.messages.take::<PrintResponse>().into()
            }
            UnionResponse::IncrementFetchCount(req) => {
                self.messages.put(req);
                self.messages.take::<IncrementFetchCountResponse>().into()
            }
            UnionResponse::AesDecrypt(req) => {
                self.messages.put(req);
                self.messages.take::<AesDecryptResponse>().into()
            }
            UnionResponse::AesEncrypt(req) => {
                self.messages.put(req);
                self.messages.take::<AesEncryptResponse>().into()
            }
            UnionResponse::UpdateResourceUsage(req) => {
                self.messages.put(req);
                self.messages.take::<UpdateResourceUsageResponse>().into()
            }
            UnionResponse::SendEmail(req) => {
                self.messages.put(req);
                self.messages.take::<SendEmailResponse>().into()
            }
            UnionResponse::RequestEmailApproval(req) => {
                self.messages.put(req);
                self.messages.take::<RequestEmailApprovalResponse>().into()
            }
            UnionResponse::CheckEmailApproval(req) => {
                self.messages.put(req);
                self.messages.take::<CheckEmailApprovalResponse>().into()
            }
            UnionResponse::Result(_) => unreachable!(), // handled in main loop
        }
    }

    fn respond_with<T: 'static>(&mut self, t: T) -> &mut Self {
        self.messages.put(t);
        self
    }

    fn received<T: 'static>(&mut self) -> T {
        self.messages.take::<T>()
    }
}

#[ctor::ctor]
fn init() {
    // Set RUST_LOG to get logs during testing
    pretty_env_logger::init();

    lit_core::utils::unix::raise_fd_limit();

    init_v8();
}

#[fixture]
fn server() -> TestServer {
    TestServer::start()
}

#[fixture]
fn client(server: TestServer) -> TestClient {
    // NB: Moving the socket file makes the client delete it once done without
    // having to deal with lifetimes all over the place.
    TestClient::new(server.socket_file)
}

#[rstest]
#[tokio::test]
async fn nop(mut client: TestClient) {
    let res = client
        .execute_js(indoc! {r#"async function main() {
    // Do nothing 
    }"#})
        .await
        .unwrap();

    assert_eq!(res.error, "");
    assert_eq!(res.success, true);

    assert_eq!(client.received::<ExecutionResult>(), res);
}

#[rstest]
#[tokio::test]
async fn console_log(mut client: TestClient) {
    client
        .respond_with(PrintResponse {})
        .execute_js(r#"async function main() { console.log("Lit Actions!") }"#)
        .await
        .unwrap();

    assert_eq!(client.received::<PrintRequest>().message, "Lit Actions!\n");
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn lit_namespace(mut client: TestClient) {
    let code = indoc! {r#"
        async function main() {
        console.log(
            Object.keys(Lit.Actions).length > 0,
            Object.keys(Lit.Headers).length === 0,

            Lit.Actions === LitActions,
            Lit.Actions === globalThis.LitActions,

            Lit.Headers === LitHeaders,
            Lit.Headers === globalThis.LitHeaders,

            Lit === globalThis.Lit,
        )
        }
    "#};

    client
        .respond_with(PrintResponse {})
        .execute_js(code)
        .await
        .unwrap();

    assert_eq!(
        client.received::<PrintRequest>().message,
        "true true true true true true true\n"
    );
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn lit_namespace_protection(mut client: TestClient) {
    let code = indoc! {r#"
        "use strict";

        const errors = [];
        const run = (fn) => { try { fn() } catch(err) { errors.push(err) } };

        async function main() {
        run(() => delete globalThis.Lit);
        run(() => delete globalThis.LitActions);
        run(() => delete globalThis.LitHeaders);

        run(() => delete Lit.Actions);
        run(() => delete Lit.Headers);

        run(() => Lit = {});
        run(() => LitActions = {});
        run(() => LitHeaders = {});

        run(() => Lit.Actions = {});
        run(() => Lit.Headers = {});

        console.log(errors.join('\n'))
        }
    "#};

    client
        .respond_with(PrintResponse {})
        .execute_js(code)
        .await
        .unwrap();

    assert_eq!(
        client.received::<PrintRequest>().message.trim_end(),
        [
            "TypeError: Cannot delete property 'Lit' of #<Window>",
            "TypeError: Cannot delete property 'LitActions' of #<Window>",
            "TypeError: Cannot delete property 'LitHeaders' of #<Window>",
            "TypeError: Cannot delete property 'Actions' of #<Object>",
            "TypeError: Cannot delete property 'Headers' of #<Object>",
            "TypeError: Cannot assign to read only property 'Lit' of object '#<Window>'",
            "TypeError: Cannot assign to read only property 'LitActions' of object '#<Window>'",
            "TypeError: Cannot assign to read only property 'LitHeaders' of object '#<Window>'",
            "TypeError: Cannot assign to read only property 'Actions' of object '#<Object>'",
            "TypeError: Cannot assign to read only property 'Headers' of object '#<Object>'",
        ]
        .join("\n")
    );
    assert!(client.received::<ExecutionResult>().success);
}

/// User code must not see the internal `__litEvalCached` helper that wraps
/// the action source for V8's eval-context code cache (CPL-264). The helper
/// deletes itself from globalThis as its first action; if that ever regresses,
/// user code would gain a string-eval primitive that bypasses
/// `--disallow-code-generation-from-strings`.
#[rstest]
#[tokio::test]
async fn lit_eval_cached_is_hidden_from_user_code(mut client: TestClient) {
    let code = indoc! {r#"
        async function main() {
        console.log(
            typeof __litEvalCached,
            typeof globalThis.__litEvalCached,
        )
        }
    "#};

    client
        .respond_with(PrintResponse {})
        .execute_js(code)
        .await
        .unwrap();

    assert_eq!(
        client.received::<PrintRequest>().message.trim_end(),
        "undefined undefined"
    );
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn js_params(mut client: TestClient) {
    {
        let code = indoc! {r#"
            async function main({ Hello, WORLD }) {
            console.log(
                Hello === "hello",
                Hello === globalThis.Hello,
                Hello === this.Hello,

                WORLD.year === 2024,
                WORLD === globalThis.WORLD,
                WORLD === this.WORLD,
            )
            }
        "#};

        client
            .respond_with(PrintResponse {})
            .execute_js(ExecutionRequest {
                code: code.into(),
                js_params: Some(b"{\"Hello\":\"hello\",\"WORLD\":{\"year\": 2024}}".into()),
                ..Default::default()
            })
            .await
            .unwrap();

        // note that js params are no longer globals, so globalThis.Hello is false and this.Hello is undefined
        assert_eq!(
            client.received::<PrintRequest>().message,
            "true false false true false false\n"
        );
        assert!(client.received::<ExecutionResult>().success);
    }

    {
        let code = indoc! {r#"
            async function main({ message }) {
                console.log(message)
            }
        "#};

        client
            .respond_with(PrintResponse {})
            .execute_js(ExecutionRequest {
                code: code.into(),
                js_params: Some(b"{\"message\":\"first\"}".into()),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(client.received::<PrintRequest>().message, "first\n");
        assert!(client.received::<ExecutionResult>().success);

        client
            .respond_with(PrintResponse {})
            .execute_js(ExecutionRequest {
                code: code.into(),
                js_params: Some(b"{\"message\":\"second\"}".into()),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(client.received::<PrintRequest>().message, "second\n");
        assert!(client.received::<ExecutionResult>().success);
    }

    // Reminder - this test is no longer valid as js Params are no longer globals
    // Check that the Lit namespace can't be modified
    // {
    //     let res = client
    //         .execute_js(ExecutionRequest {
    //             code: indoc! {r#"async function main({Lit}) {
    //                 // Do nothing
    //             }"#}.into(),
    //             js_params: Some(b"{\"Lit\":{\"Actions\": {}}}".into()),
    //             ..Default::default()
    //         })
    //         .await;

    //     assert_eq!(
    //         res.unwrap_err().to_string().lines().next().unwrap(),
    //         "Error building main worker: Error injecting params as globals: TypeError: Cannot assign to read only property 'Lit' of object '#<Window>'",
    //     );
    //     assert_eq!(client.received::<ExecutionResult>().success, false);
    // }
}

/// End-to-end proof of the code-cache fix on a *large* action — one whose
/// bundled source is well past V8's `String::kMaxHashCalcLength` (16383), the
/// regime where V8's string identity hash (and therefore Deno's code-cache
/// key) degenerates to a length-only hash.
///
/// The same action is executed twice with *different but equal-length*
/// `js_params`, asserting both halves of the fix:
///   1. **caching worked** — the second run is served from the V8 code cache
///      (compiled bytecode reused), so CPL-264 is genuinely active here;
///   2. **params were NOT cached** — each run sees its own params, even though
///      the two param payloads are byte-for-byte the same length. Before the
///      fix baked params into the cached source, equal-length payloads produced
///      equal-length sources that collided on the length-based hash, handing
///      the second run the first run's params.
#[tokio::test]
async fn large_action_caches_code_but_not_params() {
    let server = TestServer::start();
    let v8_code_cache = server.v8_code_cache.clone();
    let mut client = TestClient::new(server.socket_file);

    // ~20 KB of payload, referenced from main() so a bundler/minifier can't
    // drop it, pushes the bundled source comfortably past the 16383-char cutoff.
    let padding = "x".repeat(20 * 1024);
    let code = format!(
        r#"
        const PADDING = "{padding}";
        async function main({{ token }}) {{
            if (PADDING.length === 0) throw new Error("unreachable");
            console.log(token);
        }}
        "#
    );

    // Equal-length, different-value tokens => equal-length JSON param payloads.
    let token_a = "A".repeat(16);
    let token_b = "B".repeat(16);
    let params_a = format!(r#"{{"token":"{token_a}"}}"#);
    let params_b = format!(r#"{{"token":"{token_b}"}}"#);
    assert_eq!(
        params_a.len(),
        params_b.len(),
        "the two param payloads must be equal length to exercise the collision"
    );

    // First run: misses the V8 code cache, compiles, and stores the bytecode.
    client
        .respond_with(PrintResponse {})
        .execute_js(ExecutionRequest {
            code: code.clone(),
            js_params: Some(params_a.into_bytes()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        client.received::<PrintRequest>().message,
        format!("{token_a}\n")
    );
    assert!(client.received::<ExecutionResult>().success);

    // Second run: identical code => must HIT the V8 code cache; different
    // params => must still observe ITS OWN params.
    let hits_before = v8_code_cache.hits();
    client
        .respond_with(PrintResponse {})
        .execute_js(ExecutionRequest {
            code: code.clone(),
            js_params: Some(params_b.into_bytes()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        client.received::<PrintRequest>().message,
        format!("{token_b}\n"),
        "second execution must see its own params, not the first run's"
    );
    assert!(client.received::<ExecutionResult>().success);

    // Caching worked: the second run reused compiled bytecode.
    assert!(
        v8_code_cache.hits() > hits_before,
        "second execution of the same action must reuse compiled bytecode \
         (V8 code cache hit); hits before={}, after={}",
        hits_before,
        v8_code_cache.hits(),
    );
}

#[rstest]
#[tokio::test]
async fn set_response(mut client: TestClient) {
    client
        .respond_with(SetResponseResponse {})
        .execute_js(r#"async function main() { Lit.Actions.setResponse({response: "OK"}) }"#)
        .await
        .unwrap();

    assert_eq!(client.received::<SetResponseRequest>().response, "OK");
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn fetch(mut client: TestClient) {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    let code = formatdoc! {r#"
        async function main() {{
            await fetch("{uri}")
        }}
        "#,
        uri = &mock_server.uri()
    };

    client
        .respond_with(IncrementFetchCountResponse { fetch_count: 1 })
        .execute_js(code)
        .await
        .unwrap();

    assert_eq!(
        client.received::<IncrementFetchCountRequest>(),
        IncrementFetchCountRequest {}
    );
    assert!(client.received::<ExecutionResult>().success);
}

/// Exercises `Lit.Actions.proxiedFetch` against real Binance through a real
/// egress proxy, executed in the actual runtime in-process. Opt-in: set
/// `LIT_VENUES_TEST_PROXY` to a proxy URL (`http://user:pass@host:port`); skips
/// cleanly when unset so CI stays green and no credentials are committed.
/// (The direct-request control — geo-blocked 451 from a US IP vs. 200 via the
/// proxy — is validated separately; the test mock holds one response per op
/// type, so we keep this to a single proxied call.)
///
/// `#[ignore]` like `import_rewrite_cdn`: needs real outbound network, which the
/// default CI/dev sandbox does not grant the test process. Run with
/// `LIT_VENUES_TEST_PROXY=... cargo test -p lit-actions-tests --test integration proxied_fetch -- --ignored --nocapture`.
#[rstest]
#[ignore = "requires real network egress + LIT_VENUES_TEST_PROXY (see import_rewrite_cdn)"]
#[tokio::test]
async fn proxied_fetch(mut client: TestClient) {
    let Ok(proxy) = std::env::var("LIT_VENUES_TEST_PROXY") else {
        eprintln!("skipping proxied_fetch: set LIT_VENUES_TEST_PROXY to run");
        return;
    };

    // proxy is embedded as a JS string literal via {:?}; setResponse below only
    // ever echoes status/flags, never the proxy URL, so creds don't leak.
    let code = formatdoc! {r#"
        async function main() {{
            const viaProxy = await Lit.Actions.proxiedFetch({{
                url: "https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT",
                proxy: {proxy:?},
            }});
            const body = await viaProxy.text();
            Lit.Actions.setResponse({{ response:
                "proxyStatus=" + viaProxy.status + ";hasPrice=" + body.includes("price") }});
        }}
        "#,
        proxy = proxy
    };

    client
        .respond_with(IncrementFetchCountResponse { fetch_count: 1 })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    let response = client.received::<SetResponseRequest>().response;
    eprintln!("proxied_fetch → {response}");
    assert!(
        response.contains("proxyStatus=200"),
        "binance via proxy should return 200: {response}"
    );
    assert!(
        response.contains("hasPrice=true"),
        "expected a price field in the proxied response body: {response}"
    );

    // Drain queued op-request records so the GothamStore Drop check passes.
    let _ = client.received::<IncrementFetchCountRequest>();
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn aes_decrypt(mut client: TestClient) {
    client
        .respond_with(AesDecryptResponse { plaintext: "ignored".to_string() })
        .execute_js(
            r#"async function main() { await LitActions.Decrypt({ pkpId: "ignored", ciphertext: "456"}) }"#,
        )
        .await
        .unwrap();

    assert_eq!(
        client.received::<AesDecryptRequest>(),
        AesDecryptRequest {
            pkp_id: "ignored".to_string(),
            ciphertext: "456".to_string(),
        }
    );
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn webcrypto(mut client: TestClient) {
    let code = indoc! {r#"
        async function main() {
            const data = new TextEncoder().encode("Hello, World!")
            const hashed = await crypto.subtle.digest("SHA-256", data)
            Lit.Actions.setResponse({response: hashed.byteLength.toString()})
        }
    "#};

    client
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    assert_eq!(client.received::<SetResponseRequest>().response, "32");
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn localstorage_shouldnt_panic(mut client: TestClient) {
    let code = indoc! {r#"
        localStorage.setItem("myApp", "Deno");
    "#};

    let res = client.execute_js(code).await;

    assert_eq!(
        res.unwrap_err().to_string().lines().next().unwrap(),
        "Uncaught NotSupported: LocalStorage is not supported in this context."
    );
    assert_eq!(client.received::<ExecutionResult>().success, false);
}

#[rstest]
#[tokio::test]
async fn web_worker_shouldnt_panic(mut client: TestClient) {
    let code = indoc! {r#"
        new Worker("file:///path/to/worker.js", {type: "module"});
    "#};

    let res = client.execute_js(code).await;

    assert_eq!(
        res.unwrap_err().to_string().lines().next().unwrap(),
        "Uncaught ReferenceError: Worker is not defined"
    );
    assert_eq!(client.received::<ExecutionResult>().success, false);
}

#[rstest]
#[tokio::test]
async fn async_await(mut client: TestClient) {
    let code = indoc! {r#"
        async function main() {
            const fulfilled = await Promise.all([
                LitActions.Decrypt({pkpId: "some-key", ciphertext: "456"}),
                LitActions.setResponse({response: await "OK"})
            ])
            console.log(fulfilled)
        }
    "#};

    for _ in 0..20 {
        client
            .respond_with(PrintResponse {})
            .respond_with(AesDecryptResponse {
                plaintext: "456".to_string(),
            })
            .respond_with(SetResponseResponse {})
            .execute_js(code)
            .await
            .unwrap();

        assert_eq!(
            client.received::<AesDecryptRequest>().ciphertext,
            "456".to_string()
        );
        assert_eq!(client.received::<SetResponseRequest>().response, "OK");
        assert_eq!(
            client.received::<PrintRequest>().message,
            "[ \"456\", null ]\n"
        );
        assert!(client.received::<ExecutionResult>().success);
    }
}

#[rstest]
#[tokio::test]
async fn reference_error(mut client: TestClient) {
    let code = "async function main() { nonexisting_function() }";
    let res = client.execute_js(code).await;

    // User code now runs through `__litEvalCached` (op_eval_context) so V8's
    // script code cache is reachable for the bundled action (CPL-264). Side
    // effects on error output: the script name is the URL specifier used by
    // op_eval_context, and two wrapper frames appear at the tail. The specifier
    // is content-derived (the action's IPFS id) so distinct actions can't
    // collide on V8's length-based code-cache hash.
    let script = format!(
        "file:///user_provided_script_{}.js",
        get_lit_action_ipfs_id(code)
    );
    assert_eq!(
        res.unwrap_err().to_string(),
        formatdoc! {r#"
            Uncaught (in promise) ReferenceError: nonexisting_function is not defined
                at main ({script}:2:33)
                at {script}:6:28
                at {script}:10:11
                at globalThis.__litEvalCached (ext:lit_actions/99_patches.js:56:21)
                at <user_provided_script>:1:1
        "#, script = script}
        .trim()
    );
    assert_eq!(client.received::<ExecutionResult>().success, false);
}

#[rstest]
#[tokio::test]
async fn throw_error(mut client: TestClient) {
    {
        let code = indoc! {r#"
            async function main() {
            throw new Error("boom")
            }
        "#};
        let res = client.execute_js(code).await;

        // See `reference_error` for why the stack format differs from
        // pre-CPL-264 output and why the specifier is content-derived.
        let script = format!(
            "file:///user_provided_script_{}.js",
            get_lit_action_ipfs_id(code)
        );
        assert_eq!(
            res.unwrap_err().to_string(),
            formatdoc! {r#"
                Uncaught (in promise) Error: boom
                    at main ({script}:3:7)
                    at {script}:9:28
                    at {script}:13:11
                    at globalThis.__litEvalCached (ext:lit_actions/99_patches.js:56:21)
                    at <user_provided_script>:1:1
            "#, script = script}
            .trim(),
        );
        assert_eq!(client.received::<ExecutionResult>().success, false);
    }

    {
        let code = indoc! {r#"
            async function main() {
                throw new Error("boom")
            }
        "#};
        let res = client.execute_js(code).await;

        let script = format!(
            "file:///user_provided_script_{}.js",
            get_lit_action_ipfs_id(code)
        );
        assert_eq!(
            res.unwrap_err().to_string(),
            formatdoc! {r#"
                Uncaught (in promise) Error: boom
                    at main ({script}:3:11)
                    at {script}:9:28
                    at {script}:13:11
                    at globalThis.__litEvalCached (ext:lit_actions/99_patches.js:56:21)
                    at <user_provided_script>:1:1
            "#, script = script}
            .trim(),
        );
        assert_eq!(client.received::<ExecutionResult>().success, false);
    }
}

#[rstest]
#[tokio::test]
async fn timeout(mut client: TestClient) {
    let code = indoc! {r#"
        while (true) {}
    "#};

    let res = client
        .execute_js(ExecutionRequest {
            code: code.into(),
            timeout: Some(500),
            ..Default::default()
        })
        .await;
    let status = res.unwrap_err().downcast::<Status>().unwrap();

    assert_eq!(status.code(), Code::DeadlineExceeded);
    assert_eq!(
        status.message(),
        "Your function exceeded the maximum runtime of 500ms and was terminated."
    );
}

#[rstest]
#[tokio::test]
async fn oom(mut client: TestClient) {
    let code = indoc! {r#"
        let s = ""
        while (true) {
            s += "Hello"
        }
    "#};

    let res = client
        .execute_js(ExecutionRequest {
            code: code.into(),
            memory_limit: Some(100),
            ..Default::default()
        })
        .await;
    let status = res.unwrap_err().downcast::<Status>().unwrap();

    assert_eq!(status.code(), Code::ResourceExhausted);
    assert_eq!(
        status.message(),
        "Your function exceeded the maximum memory of 100 MB and was terminated."
    );
}

#[tokio::test]
async fn server_down() {
    let mut client_without_server = TestClient::new(temp_file::empty());

    let res = client_without_server.execute_js("ignored").await;

    assert_eq!(res.unwrap_err().to_string(), "transport error");
}

// You can run this test with `cargo test -- --ignored`, but it will fail due to
// SIGTRAP (UB of panicking in V8's C code), which can't be handled by #[should_panic].
#[rstest]
#[tokio::test]
#[ignore]
async fn panic_in_op(mut client: TestClient) {
    client
        .execute_js(r#"LitTest.op_panic("boom")"#)
        .await
        .unwrap();
}

// This includes an example for every Deno permission class except --allow-net,
// which is the only permission allowed (see fetch test).
// You can test what's allowed and denied by default via `deno repl --no-prompt`.
#[rstest]
#[tokio::test]
async fn deno_permissions(mut client: TestClient) {
    let tests = BTreeMap::from([
        (
            r#"Deno.readFileSync("test.txt")"#,
            r#"Uncaught NotCapable: Requires read access to "test.txt", run again with the --allow-read flag"#,
        ),
        (
            r#"Deno.makeTempDirSync()"#,
            r#"Uncaught NotCapable: Requires write access to <TMP>, run again with the --allow-write flag"#,
        ),
        (
            r#"Deno.env.get("SHELL")"#,
            r#"Uncaught NotCapable: Requires env access to "SHELL", run again with the --allow-env flag"#,
        ),
        (
            r#"Deno.hostname()"#,
            r#"Uncaught NotCapable: Requires sys access to "hostname", run again with the --allow-sys flag"#,
        ),
        (
            r#"Deno.kill(1234)"#,
            r#"Uncaught NotCapable: Requires run access, run again with the --allow-run flag"#,
        ),
        (
            r#"Deno.dlopen("test.dll", {})"#,
            r#"Uncaught NotCapable: Requires ffi access to "test.dll", run again with the --allow-ffi flag"#,
        ),
    ]);

    for (code, expected_err) in tests {
        let res = client.execute_js(code).await;
        assert_eq!(
            res.unwrap_err().to_string().lines().next().unwrap(),
            expected_err
        );
        assert_eq!(client.received::<ExecutionResult>().success, false);
    }
}

// Make sure we don't expose any of these versions:
// > Deno.version
// { deno: "1.41.3", v8: "12.3.219.9", typescript: "5.3.3" }
#[rstest]
#[tokio::test]
async fn deno_version(mut client: TestClient) {
    client
        .respond_with(PrintResponse {})
        .execute_js(r#"async function main() { console.log(Deno.version) }"#)
        .await
        .unwrap();

    assert_eq!(client.received::<PrintRequest>().message, "undefined\n");
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn deno_exit(mut client: TestClient) {
    for code in ["Deno.exit(1)", "Deno.exit(0)", "Deno.exit()"] {
        let res = client.execute_js(code).await;

        assert_eq!(
            res.unwrap_err().to_string().lines().next().unwrap(),
            "Uncaught PermissionDenied: 'Deno.exit' is not allowed in this context."
        );
        assert_eq!(client.received::<ExecutionResult>().success, false);
    }
}

#[rstest]
#[tokio::test]
async fn wasm(mut client: TestClient) {
    // A simple add function in WebAssembly text format compiled using:
    // wat2wasm add.wat --output=- | xxd -i
    //
    // (module
    //   (func (export "add") (param $a i32) (param $b i32) (result i32)
    //     local.get $a
    //     local.get $b
    //     i32.add
    //   )
    // )
    //
    // Source: https://docs.deno.com/runtime/reference/wasm/
    let code = indoc! {r#"
        async function main() {
        const wasmCode = new Uint8Array([
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x07, 0x01, 0x60,
            0x02, 0x7f, 0x7f, 0x01, 0x7f, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01,
            0x03, 0x61, 0x64, 0x64, 0x00, 0x00, 0x0a, 0x09, 0x01, 0x07, 0x00, 0x20,
            0x00, 0x20, 0x01, 0x6a, 0x0b
        ]);
        const wasmModule = new WebAssembly.Module(wasmCode);
        const wasmInstance = new WebAssembly.Instance(wasmModule);
            const { add } = wasmInstance.exports;
            console.log(add(123, 456));
        }
    "#};

    client
        .respond_with(PrintResponse {})
        .execute_js(code)
        .await
        .unwrap();

    assert_eq!(client.received::<PrintRequest>().message, "579\n");
    assert!(client.received::<ExecutionResult>().success);
}

// ---------------------------------------------------------------------------
// Import rewriting (CPL-209)
// ---------------------------------------------------------------------------

/// Verify that static `import` statements are rewritten to dynamic `import()`
/// calls and the imported bindings are available inside `main()`.
///
/// Uses a real jsDelivr fetch for a tiny, stable package (zod).
/// The test is marked `#[ignore]` so it doesn't run in offline CI — run it
/// explicitly via `cargo test -- --ignored import`.
#[rstest]
#[tokio::test]
#[ignore]
async fn import_rewrite_cdn(mut client: TestClient) {
    let code = indoc! {r#"
        import { z } from "zod@3.22.4/+esm";

        async function main() {
            const schema = z.string();
            const result = schema.safeParse("hello");
            Lit.Actions.setResponse({ response: String(result.success) });
        }
    "#};

    client
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    assert_eq!(client.received::<SetResponseRequest>().response, "true");
    assert!(client.received::<ExecutionResult>().success);
}

/// Verify that code without imports still works exactly as before.
#[rstest]
#[tokio::test]
async fn import_rewrite_no_imports(mut client: TestClient) {
    let code = indoc! {r#"
        async function main() {
            Lit.Actions.setResponse({ response: "no imports" });
        }
    "#};

    client
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    assert_eq!(
        client.received::<SetResponseRequest>().response,
        "no imports"
    );
    assert!(client.received::<ExecutionResult>().success);
}

// =================================================================
// Pre-warmed worker pool tests (CPL-265)
// =================================================================

/// Spin up a fresh server, hand back the pool counters, and a TestClient
/// pointed at it. Server thread keeps running after the function returns;
/// `pool_health` is a clone of the live counter `Arc`.
fn pool_test_setup() -> (TestClient, Arc<PoolHealth>, usize) {
    let server = TestServer::start();
    let pool_health = server.pool_health.clone();
    let pool_target = server.pool_target_size;
    let client = TestClient::new(server.socket_file);
    (client, pool_health, pool_target)
}

/// Wait until at least one pre-warmed worker has landed in the ready
/// channel. Polls the live `ready` gauge instead of sleeping a fixed
/// duration — snapshot bootstrap is fast locally but can be starved for
/// hundreds of ms on a loaded CI runner, which made a blind sleep flaky.
/// Returns once a worker is ready; panics if none appears within the
/// timeout (a genuine warmup failure worth surfacing).
async fn wait_for_warmup(pool_health: &Arc<PoolHealth>, target: usize) {
    if target == 0 {
        return;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if pool_health.ready() >= 1 {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no pre-warmed worker became ready within 15s (target={target})",
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// Pool hit on the warm path: after warmup, a request should be served by
/// a pre-warmed worker (hits counter increments).
#[tokio::test]
async fn pool_warm_hit() {
    let (mut client, pool_health, target) = pool_test_setup();
    if target == 0 {
        // Pool disabled (LIT_ACTIONS_POOL_SIZE=0) — nothing to assert.
        return;
    }

    wait_for_warmup(&pool_health, target).await;

    // A pre-warmed worker should serve a request and bump the hits counter.
    // Warmup timing is best-effort (see wait_for_warmup — there's no "ready
    // workers" signal), so under load the pool may not be warm after the fixed
    // wait above. Poll: issue requests until one is served from the pool, up to
    // a deadline, instead of asserting on a single request.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let hits_before = pool_health.hits();
        client
            .respond_with(PrintResponse {})
            .execute_js(r#"async function main() { console.log("warm hit") }"#)
            .await
            .unwrap();

        let _ = client.received::<PrintRequest>();
        assert!(client.received::<ExecutionResult>().success);

        let hits_after = pool_health.hits();
        if hits_after > hits_before {
            break; // served by a pre-warmed worker
        }
        assert!(
            std::time::Instant::now() < deadline,
            "expected a pool hit within the timeout (hits before={}, after={}, target={})",
            hits_before,
            hits_after,
            target,
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

/// Custom `memory_limit` requests must bypass the pool (V8 heap limits are
/// immutable post-bootstrap, so pre-warmed workers can't honour them).
#[tokio::test]
async fn pool_memory_limit_bypass() {
    let (mut client, pool_health, target) = pool_test_setup();
    if target == 0 {
        return;
    }

    wait_for_warmup(&pool_health, target).await;

    let hits_before = pool_health.hits();
    let misses_before = pool_health.misses();
    client
        .respond_with(PrintResponse {})
        .execute_js(ExecutionRequest {
            code: r#"async function main() { console.log("custom limit") }"#.into(),
            memory_limit: Some(100),
            ..Default::default()
        })
        .await
        .unwrap();

    let _ = client.received::<PrintRequest>();
    assert!(client.received::<ExecutionResult>().success);

    let hits_after = pool_health.hits();
    let misses_after = pool_health.misses();
    assert_eq!(
        hits_before, hits_after,
        "custom memory_limit must not consume a pooled worker (hits should not increment)",
    );
    assert_eq!(
        misses_before, misses_after,
        "custom memory_limit must bypass the pool entirely (misses should not increment either)",
    );
}

/// Concurrent execution: 20 in-flight requests against a pool of 10. Mix
/// of pool hits and legacy fallbacks; all must succeed without deadlock.
#[tokio::test]
async fn pool_concurrent_exhaustion() {
    use deno_runtime::deno_core::futures::future::join_all;

    let server = TestServer::start();
    let socket_path = server.socket_file.path().to_path_buf();
    // Keep `server` alive until the end of the test so the socket file
    // and runtime thread don't get torn down before requests complete.
    let _server_keepalive = server;

    let futures = (0..20).map(|i| {
        let path = socket_path.clone();
        async move { (i, raw_execute_no_op(&path).await) }
    });

    for (i, res) in join_all(futures).await {
        res.unwrap_or_else(|e| panic!("request {i} failed: {e}"));
    }
}

/// Minimal raw client helper for the concurrency test: dials the socket,
/// runs an empty `nop` action, and returns. Used so concurrent tests
/// don't share a single `TestClient` (which is sequential by design).
async fn raw_execute_no_op(socket_path: &std::path::Path) -> Result<()> {
    // Under heavy runner contention the server thread can be starved long
    // enough that establishing a fresh connection trips the 1s
    // `connect_timeout` (or the gRPC handshake races server startup),
    // surfacing as a "transport error". With 20 simultaneous dials this is
    // just a transient startup race, so retry the connect + handshake
    // briefly instead of failing the test — mirrors the retry loop in
    // `TestClient::execute_js`. Only the connection setup is retried; a
    // real execution failure is reported immediately.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match raw_execute_no_op_once(socket_path).await {
            Ok(()) => return Ok(()),
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// A single connect + handshake + execute attempt for [`raw_execute_no_op`].
async fn raw_execute_no_op_once(socket_path: &std::path::Path) -> Result<()> {
    use lit_actions_server::proto::execute_js_response::Union as UnionResp;

    let (outbound_tx, outbound_rx) = flume::bounded::<ExecuteJsRequest>(0);
    let channel = unix::connect_to_socket(socket_path).await?;
    let mut client = ActionClient::new(channel);

    let req = ExecutionRequest {
        code: "async function main() {}".into(),
        ..Default::default()
    };
    let response = client
        .execute_js(Request::new(outbound_rx.into_stream()))
        .await?;
    outbound_tx.send_async(req.into()).await?;

    let mut stream = response.into_inner();
    while let Some(resp) = stream.try_next().await? {
        match resp.union {
            Some(UnionResp::Result(res)) => {
                if !res.success {
                    bail!("execution failed: {}", res.error);
                }
                return Ok(());
            }
            Some(_) => bail!("unexpected op request from no-op action"),
            None => {}
        }
    }
    bail!("server closed connection without result")
}

/// Pool isolation: per-request `LoadedModules` must be a fresh `Arc` for
/// each pooled worker. Tested at the unit level inside the runtime crate
/// (see `runtime::tests::loaded_modules_arc_is_distinct_per_prepared_worker`)
/// — kept here as a documentation marker so future readers find it.
#[allow(dead_code)]
fn pool_isolation_doc() {}

// ---------------------------------------------------------------------------
// Email send + approval ops (plan D6/M3). checkEmailApproval verifies the
// attestation INSIDE the runtime against LIT_APPROVAL_ATTESTATION_PUBKEY —
// these tests forge the server side with a fixture key and prove the op
// accepts only a correctly signed, unexpired, id-bound attestation.

const TEST_ATTESTATION_PRIVKEY: [u8; 32] = [0x42; 32];

fn hex_str(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn install_test_attestation_pubkey() {
    let sk = k256::ecdsa::SigningKey::from_slice(&TEST_ATTESTATION_PRIVKEY).unwrap();
    let pk = hex_str(sk.verifying_key().to_encoded_point(true).as_bytes());
    // Safety: tests that touch this var all write the same fixture value.
    unsafe { std::env::set_var("LIT_APPROVAL_ATTESTATION_PUBKEY", pk) };
}

fn signed_attestation(payload_json: &str) -> String {
    use k256::ecdsa::signature::Signer;
    let sk = k256::ecdsa::SigningKey::from_slice(&TEST_ATTESTATION_PRIVKEY).unwrap();
    let sig: k256::ecdsa::Signature = sk.sign(payload_json.as_bytes());
    serde_json::json!({
        "v": "email-approval-v1",
        "alg": "secp256k1-sha256",
        "payload": payload_json,
        "sig": hex_str(&sig.to_bytes()),
    })
    .to_string()
}

fn approval_payload(approval_id: &str, expires_at_ms: u64) -> String {
    approval_payload_bound(approval_id, expires_at_ms, "")
}

fn approval_payload_bound(approval_id: &str, expires_at_ms: u64, request_hash: &str) -> String {
    serde_json::json!({
        "schema": "email-approval-v1",
        "approval_id": approval_id,
        "approver": "cfo@example.com",
        "assurance": "L2",
        "request_hash": request_hash,
        "status": "approved",
        "approved_at_ms": 1_700_000_000_000u64,
        "expires_at_ms": expires_at_ms,
    })
    .to_string()
}

const FAR_FUTURE_MS: u64 = 4_102_444_800_000; // 2100-01-01

#[rstest]
#[tokio::test]
async fn send_email(mut client: TestClient) {
    let code = indoc! {r#"
        async function main() {
            const res = await Lit.Actions.sendEmail({
                to: "ops@example.com",
                subject: "weekly report",
                text: "all venues green",
            });
            Lit.Actions.setResponse({ response: "accepted=" + res.accepted });
        }
    "#};

    client
        .respond_with(SendEmailResponse {})
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    assert_eq!(
        client.received::<SendEmailRequest>(),
        SendEmailRequest {
            to: "ops@example.com".into(),
            subject: "weekly report".into(),
            text: "all venues green".into(),
        }
    );
    assert_eq!(client.received::<SetResponseRequest>().response, "accepted=true");
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn request_email_approval(mut client: TestClient) {
    let code = indoc! {r#"
        async function main() {
            const r = await Lit.Actions.requestEmailApproval({
                to: "cfo@example.com",
                summary: "Sweep 2.5 BTC from Binance to cold storage",
                assurance: "L2",
                ttlSec: 600,
            });
            Lit.Actions.setResponse({
                response: [r.approvalId, r.otp, r.approvalUrl ?? "none"].join("|"),
            });
        }
    "#};

    client
        .respond_with(RequestEmailApprovalResponse {
            approval_id: "apr_0123abcd".into(),
            otp: "446655".into(),
            approval_url: String::new(),
        })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    assert_eq!(
        client.received::<RequestEmailApprovalRequest>(),
        RequestEmailApprovalRequest {
            to: "cfo@example.com".into(),
            summary: "Sweep 2.5 BTC from Binance to cold storage".into(),
            assurance: "L2".into(),
            ttl_sec: 600,
            request_hash: String::new(),
        }
    );
    assert_eq!(
        client.received::<SetResponseRequest>().response,
        "apr_0123abcd|446655|none"
    );
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn check_email_approval_verifies_attestation_in_runtime(mut client: TestClient) {
    install_test_attestation_pubkey();
    let attestation = signed_attestation(&approval_payload("apr_ok", FAR_FUTURE_MS));

    let code = indoc! {r#"
        async function main() {
            const r = await Lit.Actions.checkEmailApproval({ approvalId: "apr_ok" });
            Lit.Actions.setResponse({
                response: [r.approved, r.status, r.approver, r.assurance].join("|"),
            });
        }
    "#};

    client
        .respond_with(CheckEmailApprovalResponse {
            status: "approved".into(),
            attestation,
        })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    assert_eq!(
        client.received::<CheckEmailApprovalRequest>(),
        CheckEmailApprovalRequest { approval_id: "apr_ok".into(), consume: true }
    );
    assert_eq!(
        client.received::<SetResponseRequest>().response,
        "true|approved|cfo@example.com|L2"
    );
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn check_email_approval_binds_matching_operation(mut client: TestClient) {
    install_test_attestation_pubkey();
    // Attestation bound to a specific operation hash; caller passes the same.
    let attestation = signed_attestation(&approval_payload_bound("apr_b", FAR_FUTURE_MS, "0xdeadbeef"));

    let code = indoc! {r#"
        async function main() {
            const r = await Lit.Actions.checkEmailApproval({ approvalId: "apr_b", requestHash: "0xdeadbeef" });
            Lit.Actions.setResponse({ response: r.approved + "|" + r.status });
        }
    "#};

    client
        .respond_with(CheckEmailApprovalResponse { status: "approved".into(), attestation })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    client.received::<CheckEmailApprovalRequest>();
    assert_eq!(client.received::<SetResponseRequest>().response, "true|approved");
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn check_email_approval_rejects_operation_mismatch(mut client: TestClient) {
    install_test_attestation_pubkey();
    // Genuine, correctly-signed attestation for operation A — but the action is
    // about to perform operation B. The in-TEE binding must refuse it. This is
    // the core D6 guarantee: an approval for one op can't authorize another.
    let attestation = signed_attestation(&approval_payload_bound("apr_b", FAR_FUTURE_MS, "0xAAAA"));

    let code = indoc! {r#"
        async function main() {
            try {
                await Lit.Actions.checkEmailApproval({ approvalId: "apr_b", requestHash: "0xBBBB" });
                Lit.Actions.setResponse({ response: "ACCEPTED (BUG)" });
            } catch (e) {
                Lit.Actions.setResponse({ response: "rejected: " + e.message });
            }
        }
    "#};

    client
        .respond_with(CheckEmailApprovalResponse { status: "approved".into(), attestation })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    client.received::<CheckEmailApprovalRequest>();
    let response = client.received::<SetResponseRequest>().response;
    assert!(
        response.contains("does not match the expected operation"),
        "expected operation-binding rejection, got: {response}"
    );
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn check_email_approval_refuses_unbound_when_operation_expected(mut client: TestClient) {
    install_test_attestation_pubkey();
    // Unbound (notification-grade) attestation, but the action expects a bound
    // approval — must refuse so an L1 confirm can't gate a fund movement.
    let attestation = signed_attestation(&approval_payload_bound("apr_u", FAR_FUTURE_MS, ""));

    let code = indoc! {r#"
        async function main() {
            try {
                await Lit.Actions.checkEmailApproval({ approvalId: "apr_u", requestHash: "0xCCCC" });
                Lit.Actions.setResponse({ response: "ACCEPTED (BUG)" });
            } catch (e) {
                Lit.Actions.setResponse({ response: "rejected: " + e.message });
            }
        }
    "#};

    client
        .respond_with(CheckEmailApprovalResponse { status: "approved".into(), attestation })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    client.received::<CheckEmailApprovalRequest>();
    let response = client.received::<SetResponseRequest>().response;
    assert!(
        response.contains("unbound"),
        "expected unbound-refusal, got: {response}"
    );
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn check_email_approval_pending_short_circuits(mut client: TestClient) {
    let code = indoc! {r#"
        async function main() {
            const r = await Lit.Actions.checkEmailApproval({ approvalId: "apr_wait" });
            Lit.Actions.setResponse({ response: r.approved + "|" + r.status });
        }
    "#};

    client
        .respond_with(CheckEmailApprovalResponse {
            status: "pending".into(),
            attestation: String::new(),
        })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    client.received::<CheckEmailApprovalRequest>();
    assert_eq!(client.received::<SetResponseRequest>().response, "false|pending");
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn check_email_approval_rejects_tampered_attestation(mut client: TestClient) {
    install_test_attestation_pubkey();
    // Sign a real payload, then swap in a different payload (escalated assurance)
    // without re-signing — the runtime must refuse it.
    let genuine = signed_attestation(&approval_payload("apr_ok", FAR_FUTURE_MS));
    let mut envelope: serde_json::Value = serde_json::from_str(&genuine).unwrap();
    envelope["payload"] = serde_json::Value::String(
        approval_payload("apr_ok", FAR_FUTURE_MS).replace("\"L2\"", "\"L3\""),
    );
    let tampered = envelope.to_string();

    let code = indoc! {r#"
        async function main() {
            try {
                await Lit.Actions.checkEmailApproval({ approvalId: "apr_ok" });
                Lit.Actions.setResponse({ response: "ACCEPTED (BUG)" });
            } catch (e) {
                Lit.Actions.setResponse({ response: "rejected: " + e.message });
            }
        }
    "#};

    client
        .respond_with(CheckEmailApprovalResponse {
            status: "approved".into(),
            attestation: tampered,
        })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    client.received::<CheckEmailApprovalRequest>();
    let response = client.received::<SetResponseRequest>().response;
    assert!(
        response.contains("verification FAILED"),
        "expected signature failure, got: {response}"
    );
    assert!(client.received::<ExecutionResult>().success);
}

#[rstest]
#[tokio::test]
async fn check_email_approval_rejects_expired_and_misbound_attestations(mut client: TestClient) {
    install_test_attestation_pubkey();
    // Correctly signed but expired.
    let expired = signed_attestation(&approval_payload("apr_old", 1));

    let code = indoc! {r#"
        async function main() {
            try {
                await Lit.Actions.checkEmailApproval({ approvalId: "apr_old" });
                Lit.Actions.setResponse({ response: "ACCEPTED (BUG)" });
            } catch (e) {
                Lit.Actions.setResponse({ response: "rejected: " + e.message });
            }
        }
    "#};

    client
        .respond_with(CheckEmailApprovalResponse {
            status: "approved".into(),
            attestation: expired,
        })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();
    client.received::<CheckEmailApprovalRequest>();
    let response = client.received::<SetResponseRequest>().response;
    assert!(response.contains("expired"), "expected expiry failure, got: {response}");
    assert!(client.received::<ExecutionResult>().success);

    // Correctly signed for a DIFFERENT approvalId — binding must be enforced.
    let misbound = signed_attestation(&approval_payload("apr_other", FAR_FUTURE_MS));
    let code = indoc! {r#"
        async function main() {
            try {
                await Lit.Actions.checkEmailApproval({ approvalId: "apr_mine" });
                Lit.Actions.setResponse({ response: "ACCEPTED (BUG)" });
            } catch (e) {
                Lit.Actions.setResponse({ response: "rejected: " + e.message });
            }
        }
    "#};
    client
        .respond_with(CheckEmailApprovalResponse {
            status: "approved".into(),
            attestation: misbound,
        })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();
    client.received::<CheckEmailApprovalRequest>();
    let response = client.received::<SetResponseRequest>().response;
    assert!(
        response.contains("different approvalId"),
        "expected binding failure, got: {response}"
    );
    assert!(client.received::<ExecutionResult>().success);
}
