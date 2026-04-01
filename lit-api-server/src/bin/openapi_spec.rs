use lit_api_server::core::v1::endpoints::routes_with_spec;
use rocket_okapi::okapi::openapi3::{RefOr, Response, Server};

fn main() {
    let (_, mut spec) = routes_with_spec();
    spec.servers.push(Server {
        url: "/core/v1/".to_string(),
        description: Some("Lit Protocol Express API (Core v1)".to_string()),
        ..Default::default()
    });

    // Document 429 for endpoints behind the CpuAvailable guard.
    let too_many_requests = RefOr::Object(Response {
        description: "Too Many Requests \u{2014} the node is CPU-overloaded and shedding \
            load. Clients receiving this response should retry the request up to \
            five times with exponential backoff."
            .to_string(),
        ..Default::default()
    });
    if let Some(path_item) = spec.paths.get_mut("/lit_action") {
        if let Some(ref mut op) = path_item.post {
            op.responses.responses.insert("429".to_string(), too_many_requests);
        }
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&spec).expect("Failed to serialize OpenAPI spec")
    );
}
