use rocket::serde::json::Json;
use rocket::{Route, get, routes};
use serde::Serialize;

use super::dstack;

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_log: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm_config: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn routes() -> Vec<Route> {
    routes![verify]
}

#[get("/verify")]
async fn verify() -> Json<VerifyResponse> {
    match dstack::get_quote(None).await {
        Ok(q) => Json(VerifyResponse {
            success: true,
            quote: Some(q.quote),
            event_log: Some(q.event_log),
            vm_config: Some(q.vm_config),
            error: None,
        }),
        Err(e) => Json(VerifyResponse {
            success: false,
            quote: None,
            event_log: None,
            vm_config: None,
            error: Some(e),
        }),
    }
}
