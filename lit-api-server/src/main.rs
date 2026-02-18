pub mod abstractions;
pub mod core;
pub mod actions;
pub mod error;
pub mod accounts;

use rocket::fs::{FileServer, relative};
use rocket_cors::{AllowedOrigins, Method};
use std::{collections::HashSet, str::FromStr, time::Duration};
use tracing::Level;
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use crate::actions::grpc_client_pool::GrpcClientPool;
use moka::future::Cache;

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    setup_tracing().expect("Failed to setup tracing.");

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

    let r = rocket::build()
        .attach(cors)
        .mount("/core/v1/", core::v1::endpoints::routes())
        .mount("/transfer/v1/", abstractions::transfer::endpoints::routes())
        .mount(
            "/swaps/v1/",
            abstractions::intents::swaps::endpoints::routes(),
        )
        .mount("/", FileServer::from(relative!("static")))
        .manage(ipfs_cache)
        .manage(default_http_client())
        // .manage(action_store)
        .manage(GrpcClientPool::<tonic::transport::Channel>::new());
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

pub fn default_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        // .use_rustls_tls()
        .timeout(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(30)
        .build()
        .expect("Error building request client")
}
