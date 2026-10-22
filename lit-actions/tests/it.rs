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

/// A malicious action must not reach an internal service by *literal IP* — the
/// classic SSRF vector (`http://127.0.0.1:5001` → the co-located kubo IPFS
/// daemon). Deno's `deny_net` rejects it at the permission layer before any
/// socket opens (CPL-295). `op_increment_fetch_count` still fires first (the
/// JS wrapper increments before the real fetch), and the mock — bound to
/// loopback — must observe zero requests.
///
/// NB: this replaces the old happy-path `fetch` test, which fetched the
/// loopback-bound mock directly; that path is now (correctly) blocked. Real
/// outbound `fetch` success is covered by `proxied_fetch` (ignored, real net).
#[rstest]
#[tokio::test]
async fn fetch_blocks_loopback_literal_ip(mut client: TestClient) {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    // mock_server.uri() is http://127.0.0.1:<port> — a literal loopback IP.
    let code = formatdoc! {r#"
        async function main() {{
            try {{
                await fetch("{uri}");
                Lit.Actions.setResponse({{ response: "REACHED" }});
            }} catch (e) {{
                Lit.Actions.setResponse({{ response: "BLOCKED:" + e }});
            }}
        }}
        "#,
        uri = &mock_server.uri()
    };

    client
        .respond_with(IncrementFetchCountResponse { fetch_count: 1 })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    let response = client.received::<SetResponseRequest>().response;
    assert!(
        response.starts_with("BLOCKED:"),
        "loopback literal-IP fetch must be blocked, got: {response}"
    );

    let hits = mock_server.received_requests().await.unwrap_or_default();
    assert!(
        hits.is_empty(),
        "no request may reach the internal service, got {} hit(s)",
        hits.len()
    );

    let _ = client.received::<IncrementFetchCountRequest>();
    assert!(client.received::<ExecutionResult>().success);
}

/// The same block must hold when the internal address is reached by *hostname*
/// rather than literal IP. `deny_net` only inspects the URL host string, so
/// `http://localhost:<port>` slips past it — but the egress DNS resolver
/// resolves `localhost` to loopback, drops every disallowed address, and the
/// connection never happens. This is the DNS-rebinding / internal-hostname
/// layer (CPL-295).
#[rstest]
#[tokio::test]
async fn fetch_blocks_hostname_resolving_to_loopback(mut client: TestClient) {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    // Same loopback-bound mock, reached via the "localhost" hostname on its
    // port — so only DNS resolution (not the literal-IP deny rule) can catch it.
    let port = mock_server.address().port();
    let code = formatdoc! {r#"
        async function main() {{
            try {{
                await fetch("http://localhost:{port}/");
                Lit.Actions.setResponse({{ response: "REACHED" }});
            }} catch (e) {{
                Lit.Actions.setResponse({{ response: "BLOCKED:" + e }});
            }}
        }}
        "#,
        port = port
    };

    client
        .respond_with(IncrementFetchCountResponse { fetch_count: 1 })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    let response = client.received::<SetResponseRequest>().response;
    assert!(
        response.starts_with("BLOCKED:"),
        "localhost fetch must be blocked by the egress resolver, got: {response}"
    );

    let hits = mock_server.received_requests().await.unwrap_or_default();
    assert!(
        hits.is_empty(),
        "no request may reach the internal service via hostname, got {} hit(s)",
        hits.len()
    );

    let _ = client.received::<IncrementFetchCountRequest>();
    assert!(client.received::<ExecutionResult>().success);
}

/// A custom `Deno.createHttpClient()` must not be usable to tunnel around the
/// egress filter. In deno_fetch, `op_fetch_custom_client` builds its client with
/// `dns_resolver: dns::Resolver::default()` (plain getaddrinfo) — it does *not*
/// use the `fetch_dns_resolver` wired into `WorkerServiceOptions`, so a
/// `fetch(url, { client })` through a custom client would bypass the CPL-295
/// hostname/DNS-rebinding filter and reach TEE-internal services (kubo on
/// loopback, control-plane hostnames, cloud metadata). PatchDeno.js closes this
/// by deleting `Deno.createHttpClient`/`Deno.HttpClient`, and the fetch wrapper
/// strips the `client` option as defense in depth.
///
/// This mirrors `fetch_blocks_hostname_resolving_to_loopback` but drives the
/// attack through a custom client: the surface must be gone, the attempt must be
/// blocked, and the loopback-bound mock must observe zero requests.
#[rstest]
#[tokio::test]
async fn fetch_custom_http_client_cannot_bypass_egress(mut client: TestClient) {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    // Reach the loopback-bound mock by hostname — the exact surface the custom
    // client's unfiltered resolver would have reached past `deny_net`.
    let port = mock_server.address().port();
    let code = formatdoc! {r#"
        async function main() {{
            let outcome;
            try {{
                // createHttpClient is deleted, so this throws before any fetch.
                const httpClient = Deno.createHttpClient({{}});
                await fetch("http://localhost:{port}/", {{ client: httpClient }});
                outcome = "REACHED";
            }} catch (e) {{
                outcome = "BLOCKED";
            }}
            Lit.Actions.setResponse({{ response:
                "createHttpClient=" + (typeof Deno.createHttpClient) +
                ";HttpClient=" + (typeof Deno.HttpClient) +
                ";outcome=" + outcome
            }});
        }}
        "#,
        port = port
    };

    // No IncrementFetchCountResponse is queued: the throw happens before the
    // fetch wrapper runs, so `op_increment_fetch_count` never fires.
    client
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    let response = client.received::<SetResponseRequest>().response;
    assert!(
        response.contains("createHttpClient=undefined"),
        "Deno.createHttpClient must be deleted, got: {response}"
    );
    assert!(
        response.contains("HttpClient=undefined"),
        "Deno.HttpClient must be deleted, got: {response}"
    );
    assert!(
        response.contains("outcome=BLOCKED"),
        "custom-client fetch must be blocked, got: {response}"
    );

    let hits = mock_server.received_requests().await.unwrap_or_default();
    assert!(
        hits.is_empty(),
        "no request may reach the internal service via a custom client, got {} hit(s)",
        hits.len()
    );

    assert!(client.received::<ExecutionResult>().success);
}

/// The fetch wrapper must not delegate through `Function.prototype.call` or
/// any other hook an action can replace: the hook would receive the unwrapped
/// fetch as `this`, and calling it directly skips `op_increment_fetch_count`
/// and the per-session fetch quota. The hooks forward to the saved
/// `Reflect.apply` so the request itself proceeds (and is blocked at loopback)
/// on both the fixed and the vulnerable wrapper.
#[rstest]
#[tokio::test]
async fn fetch_wrapper_does_not_leak_through_call_hooks(mut client: TestClient) {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    let code = formatdoc! {r#"
        async function main() {{
            const wrappedFetch = globalThis.fetch;
            const originalCall = Function.prototype.call;
            const originalApply = Function.prototype.apply;
            const originalReflectApply = Reflect.apply;
            let leaked = 0;
            let outcome;
            const isUnwrappedFetch = (fn) =>
                typeof fn === "function" && fn !== wrappedFetch && fn.name === "fetch";
            try {{
                Function.prototype.call = function (thisArg, ...args) {{
                    if (isUnwrappedFetch(this)) leaked++;
                    return originalReflectApply(this, thisArg, args);
                }};
                Function.prototype.apply = function (thisArg, args) {{
                    if (isUnwrappedFetch(this)) leaked++;
                    return originalReflectApply(this, thisArg, args ?? []);
                }};
                Reflect.apply = function (fn, thisArg, args) {{
                    if (isUnwrappedFetch(fn)) leaked++;
                    return originalReflectApply(fn, thisArg, args);
                }};
                try {{
                    await fetch("{uri}");
                    outcome = "REACHED";
                }} catch {{
                    outcome = "BLOCKED";
                }}
            }} finally {{
                Function.prototype.call = originalCall;
                Function.prototype.apply = originalApply;
                Reflect.apply = originalReflectApply;
            }}
            Lit.Actions.setResponse({{ response: `leaked=${{leaked}} outcome=${{outcome}}` }});
        }}
        "#,
        uri = &mock_server.uri()
    };

    client
        .respond_with(IncrementFetchCountResponse { fetch_count: 1 })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    assert_eq!(
        client.received::<SetResponseRequest>().response,
        "leaked=0 outcome=BLOCKED"
    );
    let hits = mock_server.received_requests().await.unwrap_or_default();
    assert!(hits.is_empty(), "got {} hit(s)", hits.len());

    let _ = client.received::<IncrementFetchCountRequest>();
    assert!(client.received::<ExecutionResult>().success);
}

/// `Lit.Actions.proxiedFetch` is a separate egress surface (a raw `reqwest`
/// op that never touches Deno's permission engine), so it must be filtered
/// independently (CPL-295). A *direct* (proxy-less) proxiedFetch to a loopback
/// literal IP must be rejected before any socket opens, and the loopback-bound
/// mock must observe zero requests.
#[rstest]
#[tokio::test]
async fn proxied_fetch_blocks_loopback_literal_ip(mut client: TestClient) {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    // No proxy → the destination URL host (127.0.0.1) is the connect target.
    let code = formatdoc! {r#"
        async function main() {{
            try {{
                await Lit.Actions.proxiedFetch({{ url: "{uri}" }});
                Lit.Actions.setResponse({{ response: "REACHED" }});
            }} catch (e) {{
                Lit.Actions.setResponse({{ response: "BLOCKED:" + e }});
            }}
        }}
        "#,
        uri = &mock_server.uri()
    };

    client
        .respond_with(IncrementFetchCountResponse { fetch_count: 1 })
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();

    let response = client.received::<SetResponseRequest>().response;
    assert!(
        response.starts_with("BLOCKED:"),
        "loopback proxiedFetch must be blocked, got: {response}"
    );

    let hits = mock_server.received_requests().await.unwrap_or_default();
    assert!(
        hits.is_empty(),
        "no request may reach the internal service via proxiedFetch, got {} hit(s)",
        hits.len()
    );

    let _ = client.received::<IncrementFetchCountRequest>();
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

/// Normalize the volatile `__litEvalCached` frame's `line:col` in a JS stack
/// trace. That frame lives in the generated runtime file `99_patches.js`,
/// whose line numbers shift whenever that file changes (e.g. the secp256k1
/// warmup block at its tail). We still assert the frame is present and the full
/// user-code stack is exact; we just stop pinning a line number we don't own.
fn normalize_patches_line(s: &str) -> String {
    let marker = "ext:lit_actions/99_patches.js:";
    match s.find(marker) {
        Some(start) => {
            let after = start + marker.len();
            match s[after..].find(')') {
                Some(rel) => format!("{}LINE:COL{}", &s[..after], &s[after + rel..]),
                None => s.to_string(),
            }
        }
        None => s.to_string(),
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
        normalize_patches_line(&res.unwrap_err().to_string()),
        formatdoc! {r#"
            Uncaught (in promise) ReferenceError: nonexisting_function is not defined
                at main ({script}:2:33)
                at {script}:6:28
                at {script}:10:11
                at globalThis.__litEvalCached (ext:lit_actions/99_patches.js:LINE:COL)
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
            normalize_patches_line(&res.unwrap_err().to_string()),
            formatdoc! {r#"
                Uncaught (in promise) Error: boom
                    at main ({script}:3:7)
                    at {script}:9:28
                    at {script}:13:11
                    at globalThis.__litEvalCached (ext:lit_actions/99_patches.js:LINE:COL)
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
            normalize_patches_line(&res.unwrap_err().to_string()),
            formatdoc! {r#"
                Uncaught (in promise) Error: boom
                    at main ({script}:3:11)
                    at {script}:9:28
                    at {script}:13:11
                    at globalThis.__litEvalCached (ext:lit_actions/99_patches.js:LINE:COL)
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

/// Node's global `process.abort()` must not abort the server (SIGABRT), even
/// when the action holds a detached reference to the function.
#[rstest]
#[case::direct("process.abort()")]
#[case::detached("const abort = process.abort; abort()")]
#[tokio::test]
async fn node_abort_is_denied(#[case] probe: &str, mut client: TestClient) {
    assert_action_denied_and_server_alive(
        &mut client,
        probe,
        "PermissionDenied: 'process.abort' is not allowed in this context.",
    )
    .await;
}

/// Run one full action source and assert the exact response it sets. The code
/// is passed through verbatim so JS template literals/objects stay untouched.
async fn assert_action_response(client: &mut TestClient, code: &str, expected: &str) {
    client
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();
    assert_eq!(client.received::<SetResponseRequest>().response, expected);
    assert!(client.received::<ExecutionResult>().success);
}

async fn assert_action_denied_and_server_alive(
    client: &mut TestClient,
    probe: &str,
    expected: &str,
) {
    let code = formatdoc! {r#"
        async function main() {{
            let outcome;
            try {{
                {probe};
                outcome = "no error";
            }} catch (e) {{
                outcome = `${{e.name}}: ${{e.message}}`;
            }}
            Lit.Actions.setResponse({{ response: outcome }});
        }}
    "#};
    client
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();
    assert_eq!(client.received::<SetResponseRequest>().response, expected);
    assert!(client.received::<ExecutionResult>().success);

    client
        .respond_with(SetResponseResponse {})
        .execute_js(r#"async function main() { Lit.Actions.setResponse({ response: "alive" }) }"#)
        .await
        .unwrap();
    assert_eq!(client.received::<SetResponseRequest>().response, "alive");
    assert!(client.received::<ExecutionResult>().success);
}

/// Native TCPWrap methods bypass deno_core's op middleware. The original
/// bind("localhost", 0) resolves to loopback after the hostname-only check;
/// it can then listen despite deny_net. Close handles even on a RED run.
#[rstest]
#[case::localhost_listener(r#"
    const bound = handle.bind("localhost", 0);
    const listening = handle.listen(1);
    const addr = {};
    const named = handle.getsockname(addr);
    if (bound !== 0 || listening !== 0 || named !== 0 || !addr.port) {
        throw new Error(`listener probe failed: ${bound},${listening},${named},${JSON.stringify(addr)}`);
    }
"#)]
#[case::bind(r#"handle.bind("localhost", 0)"#)]
#[case::bind_with_flags(r#"handle.bindWithFlags("localhost", 0, 0)"#)]
#[case::bind6(r#"handle.bind6("localhost", 0, 0)"#)]
#[case::listen_without_bind("handle.listen(1)")]
#[case::adopt_fd("handle.open(-1)")]
#[case::borrowed_bind(r#"TCP.prototype.bind.call(handle, "localhost", 0)"#)]
#[case::prototype_descriptor(
    r#"Object.getOwnPropertyDescriptor(TCP.prototype, "bind").value.call(handle, "localhost", 0)"#
)]
#[case::constructor_alias(r#"const alias = new handle.constructor(constants.SERVER); try { alias.bind("localhost", 0); } finally { alias.close(); }"#)]
#[tokio::test]
async fn node_tcp_listeners_are_denied(#[case] operation: &str, mut client: TestClient) {
    let probe = formatdoc! {r#"
        const {{ TCP, constants }} = process.binding("tcp_wrap");
        const handle = new TCP(constants.SERVER);
        try {{ {operation}; }} finally {{ handle.close(); }}
    "#};
    assert_action_denied_and_server_alive(&mut client, &probe, LISTEN_UNSUPPORTED).await;
}

/// The SOCKET constructor and subclasses share the same native methods; do
/// not enforce this policy only on the SERVER constructor argument.
#[rstest]
#[case::client("new TCP(constants.SOCKET)")]
#[case::subclass("new (class extends TCP {})(constants.SERVER)")]
#[tokio::test]
async fn node_tcp_listener_constructor_variants_are_denied(
    #[case] constructor: &str,
    mut client: TestClient,
) {
    let probe = formatdoc! {r#"
        const {{ TCP, constants }} = process.binding("tcp_wrap");
        const handle = {constructor};
        try {{ handle.bind("localhost", 0); handle.listen(1); }} finally {{ handle.close(); }}
    "#};
    assert_action_denied_and_server_alive(&mut client, &probe, LISTEN_UNSUPPORTED).await;
}

/// Pipe listeners and fd adoption are another path to a native listener.
#[rstest]
#[case::bind(r#"handle.bind("/nonexistent-lit-actions-test-dir/listener.sock")"#)]
#[case::listen_without_bind("handle.listen(1)")]
#[case::adopt_fd("handle.open(-1)")]
#[tokio::test]
async fn node_pipe_listeners_are_denied(#[case] operation: &str, mut client: TestClient) {
    let probe = formatdoc! {r#"
        const {{ Pipe, constants }} = process.binding("pipe_wrap");
        const handle = new Pipe(constants.SERVER);
        try {{ {operation}; }} finally {{ handle.close(); }}
    "#};
    assert_action_denied_and_server_alive(&mut client, &probe, LISTEN_UNSUPPORTED).await;
}

/// Sealing the methods must not disable the distinct outbound connect ops.
#[rstest]
#[case::connect("connect")]
#[case::connect6("connect6")]
#[tokio::test]
async fn node_tcp_connect_remains_permission_checked(#[case] method: &str, mut client: TestClient) {
    let probe = formatdoc! {r#"
        const {{ TCP, TCPConnectWrap, constants }} = process.binding("tcp_wrap");
        const handle = new TCP(constants.SOCKET);
        try {{ handle.{method}(new TCPConnectWrap(), "127.0.0.1", 1234); }} finally {{ handle.close(); }}
    "#};
    assert_action_denied_and_server_alive(
        &mut client,
        &probe,
        r#"NotCapable: Requires net access to "127.0.0.1:1234", run again with the --allow-net flag"#,
    )
    .await;
}

/// User mutation cannot recover a native method from the shared prototypes.
#[rstest]
#[tokio::test]
async fn node_listener_guards_are_immutable(mut client: TestClient) {
    client
        .respond_with(SetResponseResponse {})
        .execute_js(indoc! {r#"
            async function main() {
                const { TCP } = process.binding("tcp_wrap");
                const { Pipe } = process.binding("pipe_wrap");
                for (const [ctor, methods] of [
                    [TCP, ["bind", "bind6", "bindWithFlags", "listen", "open"]],
                    [Pipe, ["bind", "listen", "open"]],
                ]) {
                    for (const method of methods) {
                        const descriptor = Object.getOwnPropertyDescriptor(ctor.prototype, method);
                        if (descriptor.writable || descriptor.configurable ||
                            Reflect.deleteProperty(ctor.prototype, method) ||
                            Reflect.set(ctor.prototype, method, () => 0) ||
                            Reflect.defineProperty(ctor.prototype, method, { value: () => 0 })) {
                            throw new Error(`Mutable listener guard: ${method}`);
                        }
                    }
                }
                Lit.Actions.setResponse({ response: "sealed" });
            }
        "#})
        .await
        .unwrap();
    assert_eq!(client.received::<SetResponseRequest>().response, "sealed");
    assert!(client.received::<ExecutionResult>().success);
}

/// Run one named test with a child-only operator allowlist. Never mutate the
/// parent environment while other tests assert production loopback denial.
async fn isolated_network_test_child(name: &str) -> bool {
    const CHILD: &str = "LIT_TEST_NETWORK_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        return true;
    }
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env(CHILD, name)
            .env("LIT_ACTIONS_EGRESS_ALLOWLIST", "127.0.0.1")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("network test child timed out")
    .unwrap();
    assert!(
        output.status.success(),
        "network child failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
    false
}

/// Exercise real outbound sockets without public-network dependencies.
#[tokio::test]
async fn node_and_deno_outbound_tcp_still_work() {
    if !isolated_network_test_child("node_and_deno_outbound_tcp_still_work").await {
        return;
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepted = tokio::spawn(async move {
        for _ in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            drop(stream);
        }
    });
    let mut client = TestClient::new(TestServer::start().socket_file);
    // Even an explicitly allowlisted address must not permit inbound sockets.
    assert_action_denied_and_server_alive(
        &mut client,
        r#"const { TCP, constants } = process.binding("tcp_wrap");
           const handle = new TCP(constants.SERVER);
           try { handle.bind("127.0.0.1", 0); } finally { handle.close(); }"#,
        LISTEN_UNSUPPORTED,
    )
    .await;
    client
        .respond_with(SetResponseResponse {})
        .execute_js(formatdoc! {r#"
            async function main() {{
                const {{ TCP, TCPConnectWrap, constants }} = process.binding("tcp_wrap");
                const handle = new TCP(constants.SOCKET);
                try {{
                    await new Promise((resolve, reject) => {{
                        const req = new TCPConnectWrap();
                        req.oncomplete = (status) => status === 0 ? resolve() : reject(new Error(`connect: ${{status}}`));
                        const status = handle.connect(req, "127.0.0.1", {port});
                        if (status !== 0) reject(new Error(`connect: ${{status}}`));
                    }});
                }} finally {{ handle.close(); }}
                const conn = await Deno.connect({{ hostname: "127.0.0.1", port: {port} }});
                conn.close();
                Lit.Actions.setResponse({{ response: "connected twice" }});
            }}
        "#})
        .await
        .unwrap();
    assert_eq!(
        client.received::<SetResponseRequest>().response,
        "connected twice"
    );
    assert!(client.received::<ExecutionResult>().success);
    tokio::time::timeout(std::time::Duration::from_secs(5), accepted)
        .await
        .unwrap()
        .unwrap();
}

/// UDP's raw-fd adoption also bypasses permissions. Use only a disposable
/// socket in an isolated child: on RED JS takes ownership, on GREEN the OS
/// closes the deliberately transferred fd when the child exits.
#[cfg(unix)]
#[tokio::test]
async fn node_udp_listeners_and_fd_adoption_are_denied() {
    use std::os::fd::IntoRawFd;

    if !isolated_network_test_child("node_udp_listeners_and_fd_adoption_are_denied").await {
        return;
    }
    let fd = std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .into_raw_fd();
    let mut client = TestClient::new(TestServer::start().socket_file);
    client
        .respond_with(SetResponseResponse {})
        .execute_js(formatdoc! {r#"
            async function main() {{
                const {{ UDP }} = process.binding("udp_wrap");
                const results = [];
                for (const attempt of [
                    (handle) => handle.bind("127.0.0.1", 0, 0),
                    (handle) => handle.bind6("127.0.0.1", 0, 0),
                    (handle) => handle.open({fd}),
                ]) {{
                    const handle = new UDP();
                    try {{
                        // Node's UDP wrapper converts NotSupported to an errno.
                        const status = attempt(handle);
                        results.push(typeof status === "number" && status < 0 ? "denied" : "allowed");
                    }} finally {{ handle.close(); }}
                }}
                Lit.Actions.setResponse({{ response: results.join(",") }});
            }}
        "#})
        .await
        .unwrap();
    assert_eq!(
        client.received::<SetResponseRequest>().response,
        "denied,denied,denied"
    );
    assert!(client.received::<ExecutionResult>().success);
    assert_action_denied_and_server_alive(
        &mut client,
        "Deno.listen({ port: 0 })",
        LISTEN_UNSUPPORTED,
    )
    .await;
    // A unix listener used to rely only on the absence of fs permissions;
    // it must be refused by the denial itself.
    assert_action_denied_and_server_alive(
        &mut client,
        r#"Deno.listen({ transport: "unix", path: "/tmp/lit-actions-denied.sock" })"#,
        LISTEN_UNSUPPORTED,
    )
    .await;
}

/// Node's TTY binding asserted on native arguments, aborting the host process:
/// TTY.isTTY(-1) and new TTY(-1) both reached `assert!(fd >= 0)` in
/// deno_node 0.186.0. Both are reachable through process.binding("tty_wrap").
#[rstest]
#[tokio::test]
async fn node_tty_binding_is_hardened(mut client: TestClient) {
    let probe = indoc! {r#"
        async function main() {
            const { TTY } = process.binding("tty_wrap");
            const isTTY = TTY.isTTY;
            const setResponse = (body) => Lit.Actions.setResponse({ response: body });
            for (const fd of [-1, -2147483648, 2147483647, 4294967295, 1.5, NaN]) {
                if (isTTY(fd) !== false) {
                    setResponse(`isTTY(${fd}) !== false`);
                    return;
                }
            }
            for (const fd of [-1, -2147483648]) {
                let threw = null;
                try {
                    new TTY(fd);
                } catch (e) {
                    threw = e;
                }
                if (!(threw instanceof RangeError)) {
                    setResponse(`new TTY(${fd}) did not throw RangeError: ${threw}`);
                    return;
                }
            }
            // Out-of-range fds must be ignored, not delegated to native code.
            // Stdio still works: fd 0 exists on the server side.
            if (typeof isTTY(0) !== "boolean") {
                setResponse("isTTY(0) is not boolean");
                return;
            }
            // The wrapper must not leak the native constructor through the
            // prototype chain: `Object.getPrototypeOf(TTY)` would otherwise be
            // the asserting native function.
            if (Object.getPrototypeOf(TTY) !== Function.prototype) {
                setResponse("TTY prototype chain leaks the native constructor");
                return;
            }
            if (TTY.prototype.constructor !== TTY) {
                setResponse("TTY.prototype.constructor is not the wrapper");
                return;
            }
            // Instances are still real native handles: the native methods
            // come from the handle's own prototype, not from the wrapper.
            const handle = new TTY(0);
            if (typeof handle.getWindowSize !== "function") {
                setResponse("TTY instance lost its native methods");
                return;
            }
            // ...but an instance must not hand back the native constructor.
            if (handle.constructor !== TTY) {
                setResponse("TTY instance leaks the native constructor");
                return;
            }
            setResponse("hardened");
        }
    "#};
    assert_action_response(&mut client, probe, "hardened").await;
}

/// Mutable call/apply hooks must never receive the guard's captured native
/// functions. Detect the leak without calling an escaped native with an invalid
/// fd, which would abort the shared test process on the vulnerable version.
/// A non-zero count can also mean deno_node itself started delegating through
/// a mutable hook inside the resolver or `process.binding`; check upstream too.
#[rstest]
#[tokio::test]
async fn node_native_guards_do_not_leak_through_call_hooks(mut client: TestClient) {
    let probe = indoc! {r#"
        async function main() {
            const { TTY } = process.binding("tty_wrap");
            const M = process.getBuiltinModule("module");
            const originalCall = Function.prototype.call;
            const originalApply = Function.prototype.apply;
            const originalReflectApply = Reflect.apply;
            let intercepted = 0;
            let ttyResult;
            let bindingResult;
            const trap = function () { intercepted++; return false; };
            try {
                Function.prototype.call = trap;
                Function.prototype.apply = trap;
                Reflect.apply = trap;
                ttyResult = TTY.isTTY(0);
                bindingResult = process.binding("tcp_wrap");
                // Exercise both delegating resolver wrappers. Native resolution
                // may reject the request; it must never delegate via our hooks.
                try { M._resolveFilename("node:path", null, false); } catch {}
                try { M._load("node:path", null, false); } catch {}
            } finally {
                Function.prototype.call = originalCall;
                Function.prototype.apply = originalApply;
                Reflect.apply = originalReflectApply;
            }
            Lit.Actions.setResponse({ response:
                `intercepted=${intercepted} tty=${typeof ttyResult} binding=${typeof bindingResult?.TCP}`
            });
        }
    "#};
    assert_action_response(
        &mut client,
        probe,
        "intercepted=0 tty=boolean binding=function",
    )
    .await;
}

/// Validation and error construction must also use captured built-ins, and
/// error names are defined rather than assigned through `Error.prototype`. A
/// fractional fd safely detects a replaced integer check without deliberately
/// passing a negative fd through to an unguarded native callback.
#[rstest]
#[tokio::test]
async fn node_native_guards_ignore_mutated_validation_builtins(mut client: TestClient) {
    let probe = indoc! {r#"
        async function main() {
            const { TTY } = process.binding("tty_wrap");
            const M = process.getBuiltinModule("module");
            const originals = {
                isInteger: Number.isInteger,
                startsWith: String.prototype.startsWith,
                String, Error, RangeError,
                errorName: Object.getOwnPropertyDescriptor(Error.prototype, "name"),
            };
            let intercepted = 0;
            let fractional, rangeError, permissionError, denied = 0, sameBinding;
            try {
                Number.isInteger = () => { intercepted++; return true; };
                String.prototype.startsWith = () => { intercepted++; return false; };
                globalThis.String = () => { intercepted++; return "tcp_wrap"; };
                globalThis.Error = function () { intercepted++; };
                globalThis.RangeError = function () { intercepted++; };
                Object.defineProperty(originals.Error.prototype, "name", {
                    get() { return "Error"; },
                    set() { intercepted++; },
                    configurable: true,
                });
                fractional = TTY.isTTY(0.5);
                try { new TTY(0.5); } catch (e) {
                    rangeError = e instanceof originals.RangeError;
                }
                try { new TTY(3); } catch (e) {
                    permissionError = e instanceof originals.Error && e.name === "NotCapable";
                }
                sameBinding = process.binding({ toString: () => "tty_wrap" }).TTY === TTY;
                for (const spec of ["internal/test/binding", "node:internal/test/binding"]) {
                    try { M._resolveFilename(spec, null, false); } catch (e) {
                        if (e instanceof originals.Error && e.name === "NotCapable") denied++;
                    }
                    try { M._load(spec, null, false); } catch (e) {
                        if (e instanceof originals.Error && e.name === "NotCapable") denied++;
                    }
                }
            } finally {
                Number.isInteger = originals.isInteger;
                originals.String.prototype.startsWith = originals.startsWith;
                globalThis.String = originals.String;
                globalThis.Error = originals.Error;
                globalThis.RangeError = originals.RangeError;
                Object.defineProperty(originals.Error.prototype, "name", originals.errorName);
            }
            Lit.Actions.setResponse({ response:
                `intercepted=${intercepted} fractional=${fractional} range=${rangeError} permission=${permissionError} binding=${sameBinding} denied=${denied}`
            });
        }
    "#};
    assert_action_response(
        &mut client,
        probe,
        "intercepted=0 fractional=false range=true permission=true binding=true denied=4",
    )
    .await;
}

/// fd > 2 needs ReadAll/WriteAll, matching Deno's own TTY constructor gate
/// (`op_tty_check_fd_permission`). Actions hold neither permission.
#[rstest]
#[tokio::test]
async fn node_tty_constructor_requires_permissions(mut client: TestClient) {
    let probe = indoc! {r#"
        async function main() {
            let outcome;
            try {
                new (process.binding("tty_wrap").TTY)(3);
                outcome = "constructed";
            } catch (e) {
                outcome = `${e.name}: ${e.message}`;
            }
            Lit.Actions.setResponse({ response: outcome });
        }
    "#};
    assert_action_response(
        &mut client,
        probe,
        "NotCapable: Requires read access to \"node:tty TTY()\", run again with the --allow-read flag",
    )
    .await;
}

/// `process.binding` must not be coerced past the hardening: deno_node looks
/// the module up with `modules[name]`, so a non-string key such as
/// `new String("tty_wrap")` hits the raw module even when a strict string
/// comparison does not. The raw module's `TTY` is the asserting native
/// constructor.
#[rstest]
#[tokio::test]
async fn node_tty_binding_key_coercion_is_hardened(mut client: TestClient) {
    let probe = indoc! {r#"
        async function main() {
            const out = [];
            for (const key of [new String("tty_wrap"), { toString: () => "tty_wrap" }]) {
                const mod = process.binding(key);
                out.push(`isTTY=${mod.TTY.isTTY(-1)}`);
                try {
                    new mod.TTY(-1);
                    out.push("ctor=allowed");
                } catch (e) {
                    out.push(`ctor=${e.name}`);
                }
            }
            const plain = process.binding("tty_wrap");
            out.push(`same=${process.binding(new String("tty_wrap")).TTY === plain.TTY}`);
            Lit.Actions.setResponse({ response: out.join(" ") });
        }
    "#};
    assert_action_response(
        &mut client,
        probe,
        "isTTY=false ctor=RangeError isTTY=false ctor=RangeError same=true",
    )
    .await;
}

/// `require()` resolution loads `NodeRequireSys`-backed state that this runtime
/// never registers: `op_require_node_module_paths` does an unconditional
/// `state.borrow::<TSys>()`, and the resulting panic cannot unwind out of the
/// V8 callback, so `createRequire(...)(...)` aborted the whole server. It is
/// also the route `internal/test/binding` used to reach raw native modules.
#[rstest]
#[tokio::test]
async fn node_require_resolution_is_denied(mut client: TestClient) {
    let probe = indoc! {r#"
        async function main() {
            const out = [];
            let mod = null;
            try {
                mod = process.getBuiltinModule("module");
                out.push(`mod=${typeof mod?.createRequire}`);
            } catch (e) {
                out.push(`mod=${e.name}`);
            }
            let req = null;
            if (mod) {
                try {
                    req = mod.createRequire("/x.js");
                    out.push("created=1");
                } catch (e) {
                    out.push(`created=${e.name}`);
                }
            }
            if (req) {
                for (const [tag, spec] of [
                    ["builtin", "zlib"],
                    ["relative", "./nope.js"],
                    ["internal", "internal/test/binding"],
                ]) {
                    try {
                        const value = req(spec);
                        out.push(`${tag}=allowed:${typeof value}`);
                    } catch (e) {
                        out.push(`${tag}=${e.name}`);
                    }
                }
            }
            Lit.Actions.setResponse({ response: out.join(" ") });
        }
    "#};
    client
        .respond_with(SetResponseResponse {})
        .execute_js(probe)
        .await
        .unwrap();
    let response = client.received::<SetResponseRequest>().response;
    assert!(client.received::<ExecutionResult>().success);
    assert!(
        response.starts_with("mod=function"),
        "module builtin should stay reachable: {response}"
    );
    assert!(
        !response.contains("allowed"),
        "require resolution must not succeed: {response}"
    );
}

/// `Module._load` resolves anything in `nativeModuleExports` without calling a
/// single op, and that table includes `internal/*`, so `createRequire` being
/// denied was not enough: `new Module("").require("internal/test/binding")`
/// returned the raw binding getter and with it every native binding module.
#[rstest]
#[tokio::test]
async fn node_internal_bindings_are_unreachable(mut client: TestClient) {
    let probe = indoc! {r#"
        async function main() {
            const out = [];
            const M = process.getBuiltinModule("module");
            const step = (tag, fn) => {
                try {
                    const value = fn();
                    out.push(`${tag}=${typeof value}`);
                } catch (e) {
                    out.push(`${tag}=${e.name}`);
                }
            };
            const m = new M("");
            step("require", () => m.require("internal/test/binding"));
            step("load", () => M._load("internal/test/binding"));
            step("resolve", () => M._resolveFilename("internal/test/binding", null, false));
            step("nodeRequire", () => m.require("node:internal/test/binding"));
            Lit.Actions.setResponse({ response: out.join(" ") });
        }
    "#};
    assert_action_response(
        &mut client,
        probe,
        "require=NotCapable load=NotCapable resolve=NotCapable nodeRequire=NotCapable",
    )
    .await;
}

/// `new Worker(specifier)` reaches `op_worker_threads_filename`, which borrows
/// `NodeRequireLoaderRc` - state this runtime never registers - so the panic
/// aborted the server before `op_create_worker` could deny the worker itself.
#[rstest]
#[tokio::test]
async fn node_worker_threads_worker_is_denied(mut client: TestClient) {
    let probe = indoc! {r#"
        async function main() {
            const out = [];
            const wt = process.getBuiltinModule("worker_threads");
            out.push(`Worker=${typeof wt.Worker}`);
            try {
                new wt.Worker("./x.js");
                out.push("worker=allowed");
            } catch (e) {
                out.push(`worker=${e.name}`);
            }
            Lit.Actions.setResponse({ response: out.join(" ") });
        }
    "#};
    assert_action_response(&mut client, probe, "Worker=function worker=NotCapable").await;
}

/// `Buffer()` and `Buffer(0)` run the deprecated-constructor path, which calls
/// `op_node_call_is_from_dependency`. That op walks the stack and then borrows
/// `NodeResolverRc` - never registered here - so the panic aborted the server
/// on an ordinary Node call, with no argument needed to trigger it.
#[rstest]
#[tokio::test]
async fn node_buffer_constructor_is_safe(mut client: TestClient) {
    let probe = indoc! {r#"
        async function main() {
            const out = [];
            out.push(`zero=${Buffer(0).length}`);
            try {
                Buffer();
                out.push("bare=allowed");
            } catch (e) {
                out.push(`bare=${e.name}`);
            }
            out.push(`from=${Buffer.from("abc").length}`);
            out.push(`alloc=${Buffer.alloc(2).length}`);
            Lit.Actions.setResponse({ response: out.join(" ") });
        }
    "#};
    assert_action_response(&mut client, probe, "zero=0 bare=TypeError from=3 alloc=2").await;
}

/// `process.constrainedMemory()` borrows `TSys`, which this runtime never
/// registers, so any call aborted the server.
#[rstest]
#[tokio::test]
async fn node_process_constrained_memory_is_safe(mut client: TestClient) {
    let probe = indoc! {r#"
        async function main() {
            const value = process.constrainedMemory();
            Lit.Actions.setResponse({
                response: `type=${typeof value} value=${value}`,
            });
        }
    "#};
    assert_action_response(&mut client, probe, "type=number value=0").await;
}

/// `Buffer.prototype.compare` checks its end offsets against `this.length` and
/// `target.length`, which a `Uint8Array` subclass can spoof with its own
/// `length` getter. The native compare op then sliced out of bounds and
/// aborted the process. It must raise a catchable RangeError instead, and keep
/// comparing valid ranges correctly.
#[rstest]
#[tokio::test]
async fn node_buffer_compare_spoofed_length_is_denied(mut client: TestClient) {
    let probe = indoc! {r#"
        async function main() {
            const out = [];
            class SpoofedSource extends Uint8Array {
                get length() { return 1000; }
            }
            try {
                Buffer.prototype.compare.call(
                    new SpoofedSource(1),
                    Buffer.from([1, 2, 3]),
                    1,
                    3,
                    0,
                    1000,
                );
                out.push("sourceEnd: allowed");
            } catch (e) {
                out.push(`sourceEnd: ${e.name}`);
            }
            class SpoofedTarget extends Uint8Array {
                get length() { return 1000; }
            }
            try {
                Buffer.prototype.compare.call(
                    Buffer.from([1]),
                    new SpoofedTarget(1),
                    0,
                    1000,
                );
                out.push("targetEnd: allowed");
            } catch (e) {
                out.push(`targetEnd: ${e.name}`);
            }
            out.push(`valid=${Buffer.from("abc").compare(Buffer.from("abd"), 0, 3, 0, 3)}`);
            out.push(`static=${Buffer.compare(Buffer.from("b"), Buffer.from("a"))}`);
            Lit.Actions.setResponse({ response: out.join(" ") });
        }
    "#};
    assert_action_response(
        &mut client,
        probe,
        "sourceEnd: RangeError targetEnd: RangeError valid=-1 static=1",
    )
    .await;
}

const VSOCK_UNSUPPORTED: &str =
    "NotSupported: The vsock transport is not supported in this context.";
const LISTEN_UNSUPPORTED: &str =
    "NotSupported: Listening sockets are not supported in this context.";

/// Issue #116: these Deno APIs reach ops that call
/// `deno_features::FeatureChecker::check_or_exit` for a feature the sandbox
/// never enables, and the default exit callback is `std::process::exit(70)`.
/// One call from any action used to terminate the whole lit-actions server,
/// along with every other tenant's in-flight execution. The `lit_actions`
/// extension middleware now replaces those ops, so the action gets a catchable
/// `NotSupported` error and the same server keeps serving requests.
///
/// Without the fix, the gated cases do not fail an assertion: the test process
/// itself exits with status 70.
///
/// `Deno.serve` reaches the same listen ops, but in this runtime it throws a
/// SyntaxError first (its lazily loaded `ext:deno_http/00_serve.ts` is
/// untranspiled TypeScript), so it has no case here.
#[rstest]
#[case::vsock_connect(
    r#"await Deno.connect({ transport: "vsock", cid: 2, port: 1234 })"#,
    VSOCK_UNSUPPORTED
)]
#[case::vsock_listen(
    r#"Deno.listen({ transport: "vsock", cid: 2, port: 1234 })"#,
    VSOCK_UNSUPPORTED
)]
#[case::tcp_listen_reuse_port(
    r#"Deno.listen({ port: 1234, reusePort: true })"#,
    LISTEN_UNSUPPORTED
)]
#[case::tls_listen_reuse_port(
    r#"Deno.listenTls({ port: 1234, reusePort: true, cert: pem("CERTIFICATE"), key: pem("PRIVATE KEY") })"#,
    LISTEN_UNSUPPORTED
)]
// Not feature-gated, but the replaced op also serves plain listening.
// Before this change it failed with NotCapable (0.0.0.0/8 is in deny_net).
#[case::tcp_listen(r#"Deno.listen({ port: 1234 })"#, LISTEN_UNSUPPORTED)]
// Control: outbound TCP is not replaced and still goes through deny_net.
#[case::tcp_connect_still_permission_checked(
    r#"await Deno.connect({ hostname: "127.0.0.1", port: 1234 })"#,
    r#"NotCapable: Requires net access to "127.0.0.1:1234", run again with the --allow-net flag"#
)]
#[tokio::test]
async fn feature_gated_deno_apis_throw_instead_of_exiting(
    #[case] probe: &str,
    #[case] expected: &str,
    mut client: TestClient,
) {
    // `pem` builds PEM framing only. deno_tls parses PEM blocks without
    // validating the DER inside, so no key material is needed to reach the
    // listen op through `Deno.listenTls`.
    let code = formatdoc! {r#"
        async function main() {{
            const pem = (label) => `-----BEGIN ${{label}}-----\nAAAA\n-----END ${{label}}-----\n`;
            let outcome;
            try {{
                {probe};
                outcome = "no error";
            }} catch (e) {{
                outcome = `${{e.name}}: ${{e.message}}`;
            }}
            Lit.Actions.setResponse({{ response: outcome }});
        }}
    "#};

    client
        .respond_with(SetResponseResponse {})
        .execute_js(code)
        .await
        .unwrap();
    assert_eq!(client.received::<SetResponseRequest>().response, expected);
    assert!(client.received::<ExecutionResult>().success);

    // The server process survived and still serves the next request.
    client
        .respond_with(SetResponseResponse {})
        .execute_js(r#"async function main() { Lit.Actions.setResponse({ response: "alive" }) }"#)
        .await
        .unwrap();
    assert_eq!(client.received::<SetResponseRequest>().response, "alive");
    assert!(client.received::<ExecutionResult>().success);
}

/// The other ops that reach `check_or_exit` (`op_net_listen_udp`,
/// `op_net_listen_unixpacket`, `op_kv_database_open`, `op_cron_create` and
/// `op_create_worker`) are replaced too, but no API visible to an action leads
/// to them, so the test above cannot reach them. `Deno.listenDatagram`,
/// `Deno.openKv` and `Deno.cron` are unstable namespace members that are never
/// installed, and PatchDeno deletes `Worker`. This pins that, so whoever
/// exposes one of them also adds a case to the test above.
#[rstest]
#[tokio::test]
async fn feature_gated_entry_points_are_not_exposed(mut client: TestClient) {
    client
        .respond_with(SetResponseResponse {})
        .execute_js(indoc! {r#"
            async function main() {
                const entryPoints = [Deno.listenDatagram, Deno.openKv, Deno.cron, globalThis.Worker];
                Lit.Actions.setResponse({ response: entryPoints.map((f) => typeof f).join(",") });
            }
        "#})
        .await
        .unwrap();
    assert_eq!(
        client.received::<SetResponseRequest>().response,
        "undefined,undefined,undefined,undefined"
    );
    assert!(client.received::<ExecutionResult>().success);
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

/// The snapshot-build-time secp256k1 warmup (tail of `99_patches.js`) must run
/// successfully and survive into every fresh, one-shot per-request isolate.
/// `__litSecp256k1Warmed` is set true at build time only if the ethers sign
/// path executed end-to-end. If ethers' API drifts (e.g. `SigningKey` or
/// `signDigest` renamed) or the warmup throws, the marker is false and signing
/// actions silently pay the cold base-point precompute (~100-150ms on the prod
/// TEE) on every request. Assert the marker here so that regression is a loud
/// CI failure, not a silent perf cliff. (That the precompute *table itself*
/// survives serialization was validated separately via an A/B latency harness;
/// this guards the warmup from quietly becoming a no-op.)
#[rstest]
#[tokio::test]
async fn secp256k1_precompute_warmed_in_snapshot(mut client: TestClient) {
    client
        .respond_with(SetResponseResponse {})
        .execute_js(
            r#"async function main() { Lit.Actions.setResponse({ response: String(globalThis.__litSecp256k1Warmed) }) }"#,
        )
        .await
        .unwrap();

    assert_eq!(
        client.received::<SetResponseRequest>().response,
        "true",
        "secp256k1 warmup did not run at snapshot-build time; signing actions will pay \
         the cold base-point precompute. Check the warmup block at the tail of \
         lit-actions/ext/js/99_patches.js (ethers SigningKey/signDigest API drift?)."
    );
    assert!(client.received::<ExecutionResult>().success);
}

/// Runs the complete Keychain decryptor bundle in the actual Deno worker. The
/// fixture's key derivation is synthetic; this is not a live TEE attestation test.
/// Start lit-agent-keychain/scripts/runtime-vector.ts and set KEYCHAIN_RUNTIME_VECTOR.
#[rstest]
#[tokio::test]
#[ignore = "requires the Keychain runtime fixture server"]
async fn keychain_encrypted_release(mut client: TestClient) {
    let path = std::env::var("KEYCHAIN_RUNTIME_VECTOR").unwrap();
    let trace_path = format!("{path}.trace");
    let trace_file = std::fs::File::create(&trace_path).unwrap();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(std::sync::Mutex::new(trace_file))
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();
    let vector: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut request: ExecutionRequest = vector["code"].as_str().unwrap().to_owned().into();
    request.js_params = Some(serde_json::to_vec(&vector["params"]).unwrap());
    client
        .respond_with(IncrementFetchCountResponse { fetch_count: 1 })
        .respond_with(GetLitActionPublicKeyResponse {
            public_key: vector["authorityPublicKey"].as_str().unwrap().into(),
        })
        .respond_with(GetLitActionPrivateKeyResponse {
            secret: vector["privateKey"].as_str().unwrap().into(),
        })
        .respond_with(SetResponseResponse {});
    let result = client.execute_js(request).await.unwrap();
    assert!(result.success);
    let response = client.received::<SetResponseRequest>().response;
    let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(
        parsed["ok"], true,
        "runtime rejected Keychain bundle: {response}"
    );
    assert!(parsed["result"]["payload"]["sealed"]["ciphertext"].is_string());
    assert!(!response.contains(vector["expected"].as_str().unwrap()));
    std::fs::write(format!("{path}.response"), response).unwrap();
    client.received::<IncrementFetchCountRequest>();
    client.received::<GetLitActionPublicKeyRequest>();
    client.received::<GetLitActionPrivateKeyRequest>();
    client.received::<ExecutionResult>();
    let trace = std::fs::read_to_string(trace_path).unwrap();
    assert!(
        !trace.contains(
            vector["privateKey"]
                .as_str()
                .unwrap()
                .trim_start_matches("0x")
        ),
        "private action key leaked to tracing"
    );
    assert!(
        !trace.contains(vector["expected"].as_str().unwrap()),
        "protected plaintext leaked to tracing"
    );
}
