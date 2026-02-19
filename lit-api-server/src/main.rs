use lit_api_server::accounts;
use lit_api_server::accounts::signer_pool::start_signer_pool;
use lit_api_server::actions::grpc::GrpcClientPool;
use lit_api_server::config;
use lit_api_server::core;
use lit_api_server::dstack;
use lit_api_server::utils::chain_info::Chain;
use moka::future::Cache;
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::{State, get, routes, uri};
use rocket_cors::{AllowedOrigins, Method};
use rocket_okapi::okapi::openapi3::{OpenApi, Server};
use rocket_okapi::swagger_ui::{SwaggerUIConfig, make_swagger_ui};
use std::{collections::HashSet, str::FromStr, sync::Arc, time::Duration};

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
            opentelemetry::{KeyValue, global},
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

                let otel_log_layer = ContextAwareOtelLogLayer::new(&logger_provider);
                let subscriber = lit_observability::init_subscriber(&obs.log_level)
                    .expect("Failed to setup tracing")
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

    // 1gb max capacity
    let ipfs_cache: Cache<String, String> = Cache::builder()
        .weigher(|_key, value: &String| -> u32 { value.len().try_into().unwrap_or(u32::MAX) })
        .max_capacity(1024 * 1024 * 1024)
        .build();

    let (core_routes, openapi_spec) = core::v1::endpoints::routes_with_spec();

    let mut r = rocket::build()
        .attach(cors)
        .mount(
            "/",
            routes![openapi_json, openapi_json_redirect, swagger_ui_redirect],
        )
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
        // .manage(action_store)
        .manage(GrpcClientPool::<tonic::transport::Channel>::new())
        .manage(signer_pool);

    {
        // /attestation at root — per Phala Get Attestation
        r = r
            .mount("/", dstack::v1::endpoints::attestation_routes())
            .mount("/dstack/v1/", dstack::v1::endpoints::routes());
    }

    r.launch().await?;

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

    Ok(())
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
