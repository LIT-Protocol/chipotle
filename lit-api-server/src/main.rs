pub mod abstractions;
pub mod accounts;
pub mod actions;
pub mod config;
pub mod core;
pub mod error;
#[cfg(feature = "phala")]
pub mod phala;

use crate::actions::grpc::GrpcClientPool;
use moka::future::Cache;
use rocket::State;
use rocket::get;
use rocket::response::Redirect;
use rocket::routes;
use rocket::serde::json::Json;
use rocket::uri;
use rocket_cors::{AllowedOrigins, Method};
use rocket_okapi::okapi::openapi3::{OpenApi, Server};
use rocket_okapi::swagger_ui::SwaggerUIConfig;
use rocket_okapi::swagger_ui::make_swagger_ui;
use std::{collections::HashSet, str::FromStr, time::Duration};
use tracing::Level;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[rocket::main]
#[allow(clippy::result_large_err)]
async fn main() -> Result<(), rocket::Error> {
    setup_tracing().expect("Failed to setup tracing.");

    if let Err(e) = config::init_config() {
        eprintln!("Failed to initialize node configuration: {:?}. Exiting.", e);
        std::process::exit(1);
    }

    if let Err(e) = accounts::signable_contract::init_signing_client() {
        eprintln!("Failed to initialize signing client: {:?}. Exiting.", e);
        std::process::exit(1);
    }

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

    // let local_rt = tokio::runtime::Builder::new_multi_thread()
    //     .thread_name("tasks")
    //     // .worker_threads(32 * num_cpus::get_physical())
    //     .worker_threads(32 * 2)
    //     .enable_all()
    //     .build()
    //     .expect("create tokio runtime");

    // for lit-actions jobs.
    // let action_store = local_rt.block_on(async {
    //     let db_path = format!("actions_queue.db");
    //     crate::actions::ActionStore::new(&db_path) // or new_in_memory() for file-less SQLite
    //         .await
    //         .expect("failed to create action store")
    // });

    // 1gb max capacity
    let ipfs_cache: Cache<String, String> = Cache::builder()
        .weigher(|_key, value: &String| -> u32 { value.len().try_into().unwrap_or(u32::MAX) })
        .max_capacity(1024 * 1024 * 1024)
        .build();

    let (core_routes, openapi_spec) = core::v1::endpoints::routes_with_spec();

    let r = rocket::build()
        .attach(cors)
        .mount(
            "/",
            routes![openapi_json, openapi_json_redirect, swagger_ui_redirect],
        )
        .mount("/core/v1/", core_routes)
        .mount("/transfer/v1/", abstractions::transfer::endpoints::routes())
        .mount(
            "/swaps/v1/",
            abstractions::intents::swaps::endpoints::routes(),
        )
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
        .manage(GrpcClientPool::<tonic::transport::Channel>::new());

    #[cfg(feature = "phala")]
    {
        r = r.mount("/phala/v1/", phala::v1::endpoints::routes());
    }

    r.launch().await?;
    Ok(())
}

fn setup_tracing() -> Result<(), anyhow::Error> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("trace"))
        .add_directive("hyper=warn".parse()?)
        .add_directive("h2=warn".parse()?) // protobuf
        .add_directive("ethers_providers::rpc::provider=warn".parse()?); // listeners

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::TRACE)
        .with_env_filter(env_filter)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
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
        // .use_rustls_tls()
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
