use lit_api_server::accounts;
use lit_api_server::accounts::chain_config::start_chain_config;
use lit_api_server::accounts::signer_pool::start_signer_pool;
use lit_api_server::actions::grpc::GrpcClientPool;
use lit_api_server::config;
use lit_api_server::core;
use lit_api_server::core::v1::guards::cpu_overload::CpuOverloadMonitor;
use lit_api_server::dstack;
use lit_api_server::observability;
use lit_api_server::restart::{RestartHandle, start_server_trigger_listener};
use lit_api_server::stripe;
use lit_api_server::utils::chain_info::Chain;
use moka::future::Cache;
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::{State, get, routes, uri};
use rocket_cors::{AllowedOrigins, Method};
use rocket_okapi::okapi::openapi3::{OpenApi, Server};
use rocket_okapi::swagger_ui::{SwaggerUIConfig, make_swagger_ui};
use std::{collections::HashSet, str::FromStr, sync::Arc, time::Duration};
use tokio::sync::mpsc;

/// Maximum number of restarts allowed within `RESTART_WINDOW`.
/// Once this limit is reached, the process exits to avoid an infinite restart loop.
const MAX_RESTARTS: u64 = 3;
/// Time window (in seconds) for restart loop protection.
const RESTART_WINDOW: Duration = Duration::from_secs(60);

// The default signer count for a new instance when contracts are deployed.
// Note that if the signers aren't funded, nothing will work until an admin sets the default api payer.
#[rocket::main]
#[allow(clippy::result_large_err)]
async fn main() -> Result<(), rocket::Error> {
    let obs = match config::read_observability_config() {
        Ok(obs) => obs,
        Err(e) => {
            eprintln!("Failed to read observability config: {e}. Exiting.");
            std::process::exit(1);
        }
    };

    // Initialize the primary tracing subscriber (stdout + privacy filtering).
    // When built with --features otlp, also initializes OTLP providers and wires
    // tracing events into the OTel log pipeline via ContextAwareOtelLogLayer.
    #[cfg(not(feature = "otlp"))]
    {
        let subscriber =
            lit_observability::init_subscriber(&obs.log_level).expect("Failed to setup tracing");
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");
    }

    #[cfg(feature = "otlp")]
    let _otlp_providers = {
        use lit_observability::{
            logging::ContextAwareOtelLogLayer,
            opentelemetry::{KeyValue, global, trace::TracerProvider},
            opentelemetry_sdk::{Resource, propagation::TraceContextPropagator, trace as sdktrace},
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
        };
        use tracing_subscriber::layer::SubscriberExt;

        let otel_resource = Resource::new(vec![KeyValue::new(SERVICE_NAME, "lit-api-server")]);
        match lit_observability::create_providers(
            &obs.telemetry_endpoint,
            otel_resource.clone(),
            sdktrace::Config::default().with_resource(otel_resource),
        )
        .await
        {
            Ok((tracing_provider, metrics_provider, logger_provider)) => {
                global::set_text_map_propagator(TraceContextPropagator::new());
                global::set_tracer_provider(tracing_provider.clone());
                global::set_meter_provider(metrics_provider.clone());
                lit_observability::metrics::install_recorder();

                let tracer = tracing_provider.tracer("lit-api-server");
                let otel_trace_layer =
                    lit_observability::tracing_opentelemetry::layer().with_tracer(tracer);
                let otel_log_layer = ContextAwareOtelLogLayer::new(&logger_provider);
                let subscriber = lit_observability::init_subscriber(&obs.log_level)
                    .expect("Failed to setup tracing")
                    .with(otel_trace_layer)
                    .with(otel_log_layer);
                tracing::subscriber::set_global_default(subscriber)
                    .expect("setting default subscriber failed");

                Some((tracing_provider, metrics_provider, logger_provider))
            }
            Err(e) => {
                eprintln!("OTLP init failed ({e}), falling back to stdout-only logging");
                let subscriber = lit_observability::init_subscriber(&obs.log_level)
                    .expect("Failed to setup tracing");
                tracing::subscriber::set_global_default(subscriber)
                    .expect("setting default subscriber failed");
                None
            }
        }
    };

    if !cfg!(feature = "production") {
        tracing::warn!(
            "THIS IS INSECURE! lit-api-server was not built with `--features production`"
        );
    }

    if let Err(e) = config::init_config() {
        eprintln!("Failed to initialize node configuration: {:?}. Exiting.", e);
        std::process::exit(1);
    }

    for chain in Chain::all_chains() {
        let info = chain.info();
        let rpc_url = chain.rpc_url();
        tracing::debug!(
            chain = %info.chain_name,
            chain_id = info.chain_id,
            "RPC: {}",
            rpc_url
        );
    }

    if let Err(e) = accounts::signable_contract::init_chain_clients().await {
        eprintln!("Failed to initialize signing client: {:?}. Exiting.", e);
        std::process::exit(1);
    }

    let signer_pool = match start_signer_pool().await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("Failed to start signer pool: {:?}. Exiting.", e);
            std::process::exit(1);
        }
    };

    let signer_pool = Arc::new(signer_pool);

    let chain_config = match start_chain_config().await {
        Ok(cfg) => Arc::new(cfg),
        Err(e) => {
            eprintln!("Failed to start chain config: {:?}. Exiting.", e);
            std::process::exit(1);
        }
    };

    let cpu_monitor = CpuOverloadMonitor::start();
    let stripe_state = stripe::init();

    // Initialize global singletons once, outside the restart loop, so they
    // aren't re-initialized (and don't re-log) on every Rocket rebuild.
    accounts::blockchain_cache::init();

    // IPFS cache lives outside the restart loop so warm entries survive restarts.
    let ipfs_cache: Cache<String, Arc<String>> = Cache::builder()
        .weigher(|_key, value: &Arc<String>| -> u32 { value.len().try_into().unwrap_or(u32::MAX) })
        .max_capacity(1024 * 1024 * 1024) // 1 GB
        .build();

    // Restart metrics: total restart count for logging.
    let mut restart_count: u64 = 0;

    // Restart loop: each iteration builds and launches a fresh Rocket instance.
    // Long-lived background services (signer pool, chain config, CPU monitor, IPFS cache)
    // are preserved across restarts.
    //
    //   main() -> init -> loop { build Rocket -> ignite -> launch <-> restart_rx } -> exit
    //                      ^                                    |
    //                      +------------------------------------+
    //                             restart signal received
    let (restart_tx, mut restart_rx) = mpsc::channel::<()>(1);

    // Start the on-chain event listener that watches for ServerTriggered events
    // from the contract owner and sends restart signals via the channel.
    start_server_trigger_listener(RestartHandle::new(restart_tx.clone()));

    // Restart loop protection: track timestamps of recent restarts.
    let mut restart_timestamps: Vec<std::time::Instant> = Vec::new();

    let mut server_error: Option<rocket::Error> = None;

    loop {
        let r = build_rocket(
            signer_pool.clone(),
            chain_config.clone(),
            cpu_monitor.clone(),
            stripe_state.clone(),
            ipfs_cache.clone(),
        );

        let rocket = match r.ignite().await {
            Ok(rocket) => rocket,
            Err(e) => {
                tracing::error!("Failed to ignite Rocket: {e}. Exiting restart loop.");
                server_error = Some(e);
                break;
            }
        };
        let shutdown = rocket.shutdown();
        let mut server_handle = tokio::spawn(rocket.launch());

        tokio::select! {
            res = &mut server_handle => {
                match res {
                    Ok(Ok(_)) => tracing::info!("Server exited cleanly."),
                    Ok(Err(e)) => {
                        tracing::error!("Server exited with error: {e}");
                        server_error = Some(e);
                    }
                    Err(e) => {
                        // A panic in the Rocket task is fatal — exit non-zero so
                        // supervisors (systemd, k8s, etc.) detect the crash.
                        tracing::error!("Server task panicked: {e}. Exiting.");
                        std::process::exit(1);
                    }
                }
                break;
            }
            msg = restart_rx.recv() => {
                if msg.is_none() {
                    // All senders dropped, channel closed. Exit the loop.
                    tracing::warn!("Restart channel closed. Shutting down.");
                    shutdown.notify();
                    await_server_handle(server_handle, &mut server_error).await;
                    break;
                }

                restart_count += 1;
                let count = restart_count;
                let now = std::time::Instant::now();
                restart_timestamps.push(now);

                // Evict timestamps outside the window.
                restart_timestamps.retain(|ts| now.duration_since(*ts) < RESTART_WINDOW);

                tracing::info!(
                    restart_count = count,
                    restarts_in_window = restart_timestamps.len(),
                    "Restart signal received. Shutting down current instance..."
                );

                if restart_timestamps.len() as u64 > MAX_RESTARTS {
                    tracing::error!(
                        max = MAX_RESTARTS,
                        window_secs = RESTART_WINDOW.as_secs(),
                        "Restart loop protection triggered. Too many restarts. Exiting."
                    );
                    shutdown.notify();
                    await_server_handle(server_handle, &mut server_error).await;
                    break;
                }

                shutdown.notify();
                await_server_handle(server_handle, &mut server_error).await;
                tracing::info!(restart_count = count, "Restarting Rocket...");
            }
        }
    }

    #[cfg(feature = "otlp")]
    if let Some((tracing_provider, metrics_provider, logger_provider)) = _otlp_providers {
        if let Err(e) = tracing_provider.shutdown() {
            eprintln!("Failed to shutdown tracing provider: {e}");
        }
        if let Err(e) = metrics_provider.shutdown() {
            eprintln!("Failed to shutdown metrics provider: {e}");
        }
        if let Err(e) = logger_provider.shutdown() {
            eprintln!("Failed to shutdown logger provider: {e}");
        }
    }

    if let Some(e) = server_error {
        Err(e)
    } else {
        Ok(())
    }
}

/// Await the Rocket server task after a shutdown has been signalled.
///
/// A clean shutdown yields `Ok(Ok(()))`. An `Err` from Rocket is recorded in
/// `server_error` so `main()` returns it. A `JoinError` (panic) is fatal:
/// we exit non-zero so supervisors detect the crash rather than silently
/// looping on a broken Rocket.
async fn await_server_handle(
    server_handle: tokio::task::JoinHandle<Result<rocket::Rocket<rocket::Ignite>, rocket::Error>>,
    server_error: &mut Option<rocket::Error>,
) {
    match server_handle.await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            tracing::error!("Server exited with error during shutdown: {e}");
            *server_error = Some(e);
        }
        Err(e) => {
            tracing::error!("Server task panicked during shutdown: {e}. Exiting.");
            std::process::exit(1);
        }
    }
}

fn build_rocket(
    signer_pool: Arc<lit_api_server::accounts::signer_pool::SignerPool>,
    chain_config: Arc<lit_api_server::accounts::chain_config::ChainConfig>,
    cpu_monitor: CpuOverloadMonitor,
    stripe_state: Option<Arc<stripe::StripeState>>,
    ipfs_cache: Cache<String, Arc<String>>,
) -> rocket::Rocket<rocket::Build> {
    let allowed_methods = HashSet::from([
        Method::from_str("Get").expect("Invalid method: Get"),
        Method::from_str("Options").expect("Invalid method: Options"),
        Method::from_str("Post").expect("Invalid method: Post"),
        Method::from_str("Patch").expect("Invalid method: Patch"),
    ]);

    let cors = rocket_cors::CorsOptions {
        allowed_origins: AllowedOrigins::all(),
        allowed_methods,
        allow_credentials: true,
        ..Default::default()
    }
    .to_cors()
    .expect("CORS failed to build");

    let (core_routes, openapi_spec) = core::v1::endpoints::routes_with_spec();

    let mut mounted_core_routes = core_routes.clone();
    for route in mounted_core_routes.iter_mut() {
        *route = route
            .clone()
            .map_base(|base| format!("/core/v1/{}", base))
            .expect("Failed to map base of a route.");
    }

    let metrics_fairings = lit_api_core::observability::MetricsFairings::new(mounted_core_routes);

    let mut r = rocket::build()
        .attach(observability::ObservabilityFairing::new())
        .attach(cors)
        .attach(metrics_fairings)
        .mount(
            "/",
            routes![openapi_json, openapi_json_redirect, swagger_ui_redirect],
        )
        .mount("/", core::v1::health::routes())
        .mount("/core/v1/", core_routes)
        .mount(
            "/core/v1/swagger-ui/",
            make_swagger_ui(&SwaggerUIConfig {
                url: "/core/v1/openapi.json".to_owned(),
                ..Default::default()
            }),
        )
        .manage(ipfs_cache)
        .manage(openapi_spec)
        .manage(default_http_client())
        .manage(GrpcClientPool::<tonic::transport::Channel>::new())
        .manage(signer_pool)
        .manage(chain_config)
        .manage(cpu_monitor)
        .manage(stripe_state)
        .manage(core::v1::health::LitActionsSocketPath(
            std::path::PathBuf::from(core::v1::health::LIT_ACTIONS_SOCKET),
        ));

    // /attestation at root — per Phala Get Attestation
    r = r
        .mount("/", dstack::v1::endpoints::attestation_routes())
        .mount("/dstack/v1/", dstack::v1::endpoints::routes());

    r
}

#[get("/core/v1/openapi.json")]
fn openapi_json(spec: &State<OpenApi>) -> Json<OpenApi> {
    let mut spec = spec.inner().clone();

    spec.servers.push(Server {
        url: "/core/v1/".to_string(),
        description: Some("Lit Protocol Express API (Core v1)".to_string()),
        ..Default::default()
    });

    Json(spec)
}

pub fn default_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(30)
        .build()
        .expect("Error building request client")
}

#[get("/openapi.json")]
fn openapi_json_redirect() -> Redirect {
    Redirect::permanent(uri!("/core/v1/openapi.json"))
}

#[get("/")]
fn swagger_ui_redirect() -> Redirect {
    Redirect::permanent(uri!("/core/v1/swagger-ui/"))
}
