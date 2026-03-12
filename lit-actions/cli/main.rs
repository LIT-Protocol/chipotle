use clap::Parser;
use lit_core::utils::unix::raise_fd_limit;
use tracing::{debug, error, info};

#[cfg(feature = "otlp")]
use lit_observability::opentelemetry_sdk::{logs::LoggerProvider, metrics::SdkMeterProvider};

#[derive(Debug, Parser)]
struct Args {
    #[arg(
        short,
        long,
        default_value = "/tmp/lit_actions.sock",
        help = "Path to Unix domain socket used by gRPC server"
    )]
    socket: std::path::PathBuf,
}

fn main() -> anyhow::Result<()> {
    raise_fd_limit();

    let args = Args::parse();

    let observability_rt = tokio::runtime::Runtime::new().expect("failed to create runtime");

    let observability_providers =
        observability_rt.block_on(async { init_observability().await });
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

async fn init_observability() -> ObservabilityProviders {
    use tracing_subscriber::util::SubscriberInitExt;

    let log_level =
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

    let init_stdout = || match lit_observability::init_subscriber(&log_level) {
        Ok(s) => s.init(),
        Err(e) => eprintln!("Failed to init tracing subscriber: {e}"),
    };

    #[cfg(not(feature = "otlp"))]
    {
        init_stdout();
        return ObservabilityProviders::default();
    }

    #[cfg(feature = "otlp")]
    {
        use lit_observability::{
            logging::ContextAwareOtelLogLayer,
            opentelemetry::{KeyValue, global},
            opentelemetry_sdk::{Resource, propagation::TraceContextPropagator, trace as sdktrace},
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
        };
        use tracing_subscriber::layer::SubscriberExt;

        let endpoint = std::env::var("LIT_TELEMETRY_ENDPOINT")
            .unwrap_or_else(|_| "http://127.0.0.1:4317".to_string());

        let otel_resource = Resource::new(vec![KeyValue::new(SERVICE_NAME, "lit-actions")]);

        let (tracing_provider, metrics_provider, logger_provider) =
            match lit_observability::create_providers(
                &endpoint,
                otel_resource.clone(),
                sdktrace::Config::default().with_resource(otel_resource),
            )
            .await
            {
                Ok(providers) => providers,
                Err(e) => {
                    eprintln!("OTLP init failed ({e}), falling back to stdout-only logging");
                    init_stdout();
                    return ObservabilityProviders::default();
                }
            };

        global::set_text_map_propagator(TraceContextPropagator::new());
        global::set_tracer_provider(tracing_provider);
        global::set_meter_provider(metrics_provider.clone());

        let otel_log_layer = ContextAwareOtelLogLayer::new(&logger_provider);
        match lit_observability::init_subscriber(&log_level) {
            Ok(s) => s.with(otel_log_layer).init(),
            Err(e) => eprintln!("Failed to init tracing subscriber: {e}"),
        }

        ObservabilityProviders::new(metrics_provider, logger_provider)
    }
}

#[derive(Default)]
struct ObservabilityProviders {
    #[cfg(feature = "otlp")]
    meter_provider: Option<SdkMeterProvider>,
    #[cfg(feature = "otlp")]
    logger_provider: Option<LoggerProvider>,
}

impl ObservabilityProviders {
    #[cfg(feature = "otlp")]
    fn new(meter_provider: SdkMeterProvider, logger_provider: LoggerProvider) -> Self {
        Self {
            meter_provider: Some(meter_provider),
            logger_provider: Some(logger_provider),
        }
    }

    fn shutdown(self) {
        #[cfg(feature = "otlp")]
        {
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
}
