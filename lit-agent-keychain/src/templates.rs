//! Public, content-addressed access to released action templates. Clients bundle
//! only the current release of each template; to operate a vault or secret pinned
//! to an earlier release they fetch those exact bytes here and verify the hash
//! (and the resulting CID) locally, so nothing served here is trusted.
use crate::{actions, api::ApiResult};
use rocket::{get, http::ContentType, response::content::RawJavaScript, serde::json::Json};
use serde_json::{json, Value};

#[get("/api/templates")]
pub fn index() -> ApiResult<Value> {
    let templates: serde_json::Map<String, Value> = actions::version_index()
        .into_iter()
        .map(|(name, hashes)| (name.to_owned(), json!(hashes)))
        .collect();
    Ok(Json(json!({"templates":templates})))
}
#[get("/api/templates/<hash>")]
pub fn template(hash: &str) -> Option<(ContentType, RawJavaScript<&'static str>)> {
    if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    actions::template(hash).map(|code| (ContentType::JavaScript, RawJavaScript(code)))
}
