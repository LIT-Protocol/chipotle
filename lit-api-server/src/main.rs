pub mod abstractions;
pub mod core;
pub mod actions;

use rocket::fs::{FileServer, relative};
use rocket_cors::{AllowedOrigins, Method};
use std::{collections::HashSet, str::FromStr};
use tracing::Level;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

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

    let r = rocket::build()
        .attach(cors)
        .mount("/core/v1/", core::v1::endpoints::routes())
        .mount("/transfer/v1/", abstractions::transfer::endpoints::routes())
        .mount(
            "/swaps/v1/",
            abstractions::intents::swaps::endpoints::routes(),
        )
        .mount("/", FileServer::from(relative!("static")));
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
