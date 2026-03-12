use lit_api_server::core::v1::endpoints::routes_with_spec;
use rocket_okapi::okapi::openapi3::Server;

fn main() {
    let (_, mut spec) = routes_with_spec();
    spec.servers.push(Server {
        url: "/core/v1/".to_string(),
        description: Some("Lit Protocol Express API (Core v1)".to_string()),
        ..Default::default()
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&spec).expect("Failed to serialize OpenAPI spec")
    );
}
