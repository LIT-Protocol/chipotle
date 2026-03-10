use anyhow::Result;
use clap::Parser;
use lit_core::utils::unix::raise_fd_limit;
use lit_observability::{
    opentelemetry::{KeyValue, global},
    opentelemetry_sdk::{
        Resource, logs::LoggerProvider, metrics::SdkMeterProvider,
        propagation::TraceContextPropagator, trace as sdktrace,
    },
};
use tracing::{debug, error, info};

#[derive(Debug, Parser)]
struct Args {
    #[arg(
        short,
        long,
        default_value = "/tmp/lit_actions.sock",
        help = "Path to Unix domain socket used by gRPC server"
    )]
    socket: std::path::PathBuf,

    #[arg(long, default_value = "false", help = "Enable observability export")]
    enable_observability_export: bool,
}

fn main() -> Result<()> {
    raise_fd_limit();

    let args = Args::parse();

    let observability_rt = tokio::runtime::Runtime::new().expect("failed to create runtime");

    let observability_providers = observability_rt
        .block_on(async { init_observability(&args).await })
        .expect("failed to init observability");
    debug!(?args);

    lit_actions_server::init_v8();

    info!("Listening on {:?}", args.socket);

    let main_rt = tokio::runtime::Runtime::new().expect("failed to create runtime");

    main_rt.block_on(async {
        let signal = async {
            let _ = tokio::signal::ctrl_c().await;
        };
        lit_actions_server::start_server(args.socket, Some(signal))
            .await
            .inspect_err(|e| error!("Server error: {e:?}"))
            .expect("failed to start server");
    });

    observability_providers.shutdown();

    Ok(())
}

async fn init_observability(args: &Args) -> Result<ObservabilityProviders> {
    use lit_api_core::config::LitApiConfig;
    use lit_core::config::{LitConfig, LitConfigBuilder, envs::LitEnv};
    use lit_observability::LitObservabilityConfig;
    use tracing_subscriber::util::SubscriberInitExt;

    // NB: constructing LitConfig requires lit.env, but the value isn't used by init_subscriber
    let mut builder = LitConfigBuilder::default().set_override("lit.env", LitEnv::Dev.to_string());
    builder = <LitConfig as LitObservabilityConfig>::apply_defaults(builder)?;
    let cfg = <LitConfig as LitApiConfig>::from_builder(builder)?;

    if !args.enable_observability_export {
        lit_observability::init_subscriber(&cfg)
            .map_err(|e| anyhow::anyhow!("Failed to init subscriber: {}", e))?
            .init();
        return Ok(ObservabilityProviders::default());
    }

    let otel_resource = Resource::new(vec![KeyValue::new(
        lit_observability::opentelemetry_semantic_conventions::resource::SERVICE_NAME,
        "lit-actions",
    )]);

    let (tracing_provider, metrics_provider, logger_provider) =
        lit_observability::create_providers(
            &cfg,
            otel_resource.clone(),
            sdktrace::Config::default().with_resource(otel_resource.clone()),
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create OTEL providers: {}", e))?;

    // Set globals
    global::set_text_map_propagator(TraceContextPropagator::new());
    global::set_tracer_provider(tracing_provider);
    global::set_meter_provider(metrics_provider.clone());
    lit_observability::init_subscriber(&cfg)
        .map_err(|e| anyhow::anyhow!("Failed to init subscriber: {}", e))?
        .init();

    Ok(ObservabilityProviders::new(
        metrics_provider,
        logger_provider,
    ))
}

#[derive(Default)]
struct ObservabilityProviders {
    meter_provider: Option<SdkMeterProvider>,
    logger_provider: Option<LoggerProvider>,
}

impl ObservabilityProviders {
    fn new(meter_provider: SdkMeterProvider, logger_provider: LoggerProvider) -> Self {
        Self {
            meter_provider: Some(meter_provider),
            logger_provider: Some(logger_provider),
        }
    }

    fn shutdown(self) {
        if let Some(meter_provider) = self.meter_provider
            && let Err(e) = meter_provider.shutdown()
        {
            error!("Failed to shutdown metrics provider: {:?}", e);
        }
        if let Some(logger_provider) = self.logger_provider
            && let Err(e) = logger_provider.shutdown()
        {
            error!("Failed to shutdown logger provider: {:?}", e);
        }
    }
}
