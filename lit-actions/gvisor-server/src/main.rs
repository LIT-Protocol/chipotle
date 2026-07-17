use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use clap::Parser;
use lit_actions_gvisor_server::bundle::BundleCache;
use lit_actions_gvisor_server::sandbox::{
    ProcessRuntime, RunscConfig, RunscRuntime, SandboxRuntime,
};
use lit_actions_gvisor_server::supervisor::Supervisor;
use tracing::info;

#[derive(Clone, Copy, Debug, PartialEq, clap::ValueEnum)]
enum RuntimeKind {
    /// gVisor sandbox per execution (production).
    Runsc,
    /// Plain child process, NO isolation (tests / local dev only).
    Process,
}

#[derive(Debug, Parser)]
struct Args {
    /// Path to the Unix domain socket used by the gRPC op-loop server.
    /// Deliberately distinct from the JS runner's /tmp/lit_actions.sock so
    /// both runners can serve side by side from the shared socket volume.
    #[arg(short, long, default_value = "/tmp/lit_actions_gvisor.sock")]
    socket: PathBuf,

    #[arg(long, env = "LIT_SANDBOX_RUNTIME", value_enum, default_value_t = RuntimeKind::Runsc)]
    runtime: RuntimeKind,

    /// Read-only base image rootfs for runsc sandboxes (language runtimes +
    /// the preinstalled `lit` CLI).
    #[arg(long, env = "LIT_SANDBOX_ROOTFS", required_if_eq("runtime", "runsc"))]
    rootfs: Option<PathBuf>,

    #[arg(long, default_value = "runsc")]
    runsc_path: PathBuf,

    /// gVisor platform (systrap and ptrace are both validated inside TDX).
    #[arg(long, env = "LIT_RUNSC_PLATFORM", default_value = "systrap")]
    platform: String,

    /// runsc network mode: sandbox (netstack), host, or none.
    #[arg(long, env = "LIT_RUNSC_NETWORK", default_value = "sandbox")]
    network: String,

    /// Skip per-sandbox cgroup setup (dev/tests; container-in-container
    /// hosts without a delegated cgroup subtree).
    #[arg(long, env = "LIT_RUNSC_IGNORE_CGROUPS", default_value_t = false)]
    ignore_cgroups: bool,

    /// Where unpacked content-addressed bundles are cached.
    #[arg(long, env = "LIT_BUNDLE_CACHE_DIR")]
    bundle_cache_dir: Option<PathBuf>,

    /// Directory containing the guest `lit` CLI (process runtime only;
    /// defaults to this binary's directory).
    #[arg(long, env = "LIT_GUEST_BIN_DIR")]
    guest_bin_dir: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let (subscriber, _log_guard) = lit_observability::init_subscriber(&log_level)
        .expect("Failed to init tracing subscriber (invalid RUST_LOG or filter config)");
    {
        use tracing_subscriber::util::SubscriberInitExt as _;
        subscriber.init();
    }

    let runtime: Arc<dyn SandboxRuntime> = match args.runtime {
        RuntimeKind::Runsc => Arc::new(RunscRuntime::new(RunscConfig {
            runsc_path: args.runsc_path,
            rootfs: args.rootfs.expect("clap enforces --rootfs for runsc"),
            platform: args.platform,
            network: args.network,
            ignore_cgroups: args.ignore_cgroups,
        })?),
        RuntimeKind::Process => {
            let guest_bin_dir = match args.guest_bin_dir {
                Some(dir) => dir,
                None => std::env::current_exe()
                    .context("failed to locate current executable")?
                    .parent()
                    .context("executable has no parent dir")?
                    .to_path_buf(),
            };
            Arc::new(ProcessRuntime { guest_bin_dir })
        }
    };

    let bundle_cache_dir = args
        .bundle_cache_dir
        .unwrap_or_else(|| std::env::temp_dir().join("lit-gvisor-bundles"));
    let bundle_cache = BundleCache::new(bundle_cache_dir)?;

    info!("Listening on {:?}", args.socket);
    info!("Sandbox runtime: {:?}", args.runtime);

    let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
    rt.block_on(async {
        let signal = async {
            let _ = tokio::signal::ctrl_c().await;
        };
        lit_actions_gvisor_server::start_server(
            args.socket,
            Some(signal),
            Supervisor::new(runtime, bundle_cache),
        )
        .await
    })
}
