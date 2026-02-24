//! Rocket server that serves static assets (SDKs, docs, dapps).

use rocket::fs::FileServer;

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    let static_dir = std::env::current_dir()
        .unwrap()
        .join("static");

    rocket::build()
        .mount("/", FileServer::from(static_dir))
        .launch()
        .await?;
    Ok(())
}
