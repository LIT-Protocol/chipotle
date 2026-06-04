use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use anyhow::{Result, bail};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use deno_resolver::npm::{DenoInNpmPackageChecker, ManagedNpmResolver};
use deno_runtime::{
    BootstrapOptions,
    deno_core::{ModuleCodeString, NoopModuleLoader, resolve_url_or_path},
    deno_fs::RealFs,
    deno_permissions::PermissionsContainer,
    permissions::RuntimePermissionDescriptorParser,
    tokio_util::create_basic_runtime,
    worker::{MainWorker, WorkerOptions, WorkerServiceOptions},
};
use indoc::indoc;
use lit_actions_server::{TestServer, init_v8, proto::*, unix};
use sys_traits::impls::RealSys;
use tokio_stream::StreamExt as _;
use tonic::Request;

// This client is only used for benchmarking. The real implementation is provided by lit-node.
#[derive(Debug)]
struct TestClient {
    socket_path: PathBuf,
}

impl TestClient {
    fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    async fn execute_js(
        &mut self,
        request: impl Into<ExecutionRequest>,
    ) -> Result<ExecutionResult> {
        let request = request.into();

        let (outbound_tx, outbound_rx) = flume::bounded(0);
        let channel = unix::connect_to_socket(&self.socket_path).await?;
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
                    if !res.success {
                        bail!(res.error);
                    }
                    return Ok(res);
                }
                // Handle op requests
                Some(op) => {
                    let resp = self.handle_op(op);
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
            UnionResponse::SetResponse(_) => SetResponseResponse {}.into(),
            // Answer the deferred async op instantly, in-process, so the bench
            // measures Deno's per-op event-loop / dispatch overhead rather than
            // any network round-trip to a real lit-node.
            UnionResponse::GetPrivateKey(_) => GetPrivateKeyResponse {
                secret: "0xdeadbeef".to_string(),
            }
            .into(),
            _ => unimplemented!("op not implemented"),
        }
    }
}

static SNAPSHOT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/BASE_SNAPSHOT.bin"));

const ASYNC_CODE: &str = indoc! {r#"
    async function main() {
        const numbers = Array.from({ length: 100 }, (_, i) => i);
        const doubled = numbers.map(n => n * 2);
        const sum = doubled.reduce((acc, n) => acc + n, 0);
        const result = await Promise.all([
            Promise.resolve(sum),
            Promise.resolve(doubled.length)
        ]);
        return result[0] / result[1];
    }
"#};

const OPS_CODE: &str = indoc! {r#"
    async function main() {
        LitActions.setResponse({ response: "ok" })
    }
"#};

// 100 sequential awaited deferred-async-op round-trips. Each resolves on an
// event-loop turn, so this amplifies and isolates Deno's per-op dispatch cost.
const DEFERRED_OPS_CODE: &str = indoc! {r#"
    async function main() {
        for (let i = 0; i < 100; i++) {
            await LitActions.getPrivateKey({ pkpId: "0x1234" });
        }
    }
"#};

fn lit_actions(c: &mut Criterion) {
    init_v8();

    let server = TestServer::start();
    let runtime = create_basic_runtime();
    let mut group = c.benchmark_group("Lit Actions");

    group.bench_function("Async code", |b| {
        b.to_async(&runtime).iter(|| async {
            TestClient::new(server.socket_path())
                .execute_js(black_box(ASYNC_CODE))
                .await
                .unwrap();
        })
    });

    group.bench_function("Rust ops", |b| {
        b.to_async(&runtime).iter(|| async {
            TestClient::new(server.socket_path())
                .execute_js(black_box(OPS_CODE))
                .await
                .unwrap();
        })
    });

    group.bench_function("No code", |b| {
        b.to_async(&runtime).iter(|| async {
            TestClient::new(server.socket_path())
                .execute_js(black_box(""))
                .await
                .unwrap();
        })
    });

    group.bench_function("Deferred ops x100", |b| {
        b.to_async(&runtime).iter(|| async {
            TestClient::new(server.socket_path())
                .execute_js(black_box(DEFERRED_OPS_CODE))
                .await
                .unwrap();
        })
    });

    group.finish();
}

fn vanilla_deno(c: &mut Criterion) {
    init_v8();

    let runtime = create_basic_runtime();
    let mut group = c.benchmark_group("Vanilla Deno");

    group.bench_function("Async code", |b| {
        b.to_async(&runtime).iter(|| async {
            let permission_desc_parser = Arc::new(RuntimePermissionDescriptorParser::new(RealSys));
            let services = WorkerServiceOptions::<
                DenoInNpmPackageChecker,
                ManagedNpmResolver<RealSys>,
                RealSys,
            > {
                blob_store: Default::default(),
                broadcast_channel: Default::default(),
                deno_rt_native_addon_loader: Default::default(),
                feature_checker: Default::default(),
                fs: Arc::new(RealFs),
                module_loader: Rc::new(NoopModuleLoader),
                node_services: Default::default(),
                npm_process_state_provider: Default::default(),
                permissions: PermissionsContainer::allow_all(permission_desc_parser),
                root_cert_store_provider: Default::default(),
                fetch_dns_resolver: Default::default(),
                shared_array_buffer_store: Default::default(),
                compiled_wasm_module_store: Default::default(),
                v8_code_cache: Default::default(),
                bundle_provider: Default::default(),
            };
            let options = WorkerOptions {
                bootstrap: BootstrapOptions {
                    cpu_count: 1,
                    ..Default::default()
                },
                startup_snapshot: Some(SNAPSHOT),
                ..Default::default()
            };

            let main_module =
                resolve_url_or_path("./$bench.js", Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
            let mut worker = MainWorker::bootstrap_from_options(&main_module, services, options);

            worker
                .execute_script(
                    "bench.js",
                    black_box(ModuleCodeString::from_static(ASYNC_CODE)),
                )
                .unwrap();
            worker.run_event_loop(false).await.unwrap();
        })
    });

    group.finish();
}

criterion_group!(benches, lit_actions, vanilla_deno);
criterion_main!(benches);
