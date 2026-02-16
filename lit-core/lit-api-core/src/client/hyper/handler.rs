use http_body_util::{BodyExt, Full};
use hyper::{body::Bytes, http::request::Builder};
use hyper_util::client::legacy::{Client, connect::Connect};
use serde::{Deserialize, Serialize};

use crate::context::{HEADER_KEY_X_CORRELATION_ID, TRACING};
use crate::error::{Result, http_client_err};

pub async fn handle_request<RQ, RS, C>(
    client: &Client<C, Full<Bytes>>, mut builder: Builder, req_body: RQ,
) -> Result<RS>
where
    RQ: Serialize + Send,
    RS: for<'a> Deserialize<'a> + Send,
    C: Connect + Clone + Send + Sync + 'static,
{
    // Attempt to add tracing information to the request.
    {
        let mut correlation_id: Option<String> = None;
        let _ = TRACING.try_with(|tracing| {
            let _ = correlation_id.insert(tracing.correlation_id().clone());
        });
        if let Some(correlation_id) = correlation_id {
            builder = builder.header(HEADER_KEY_X_CORRELATION_ID, correlation_id);
        }
    }

    // Serialize the request body.
    let request_body_bytes = serde_json::to_vec(&req_body)
        .map_err(|e| http_client_err(e, Some("Unable to serialize request".into())))?;

    // Finish building the request.
    let request = builder
        .body(Full::new(Bytes::from(request_body_bytes)))
        .map_err(|e| http_client_err(e, Some("Unable to build request".into())))?;

    // Send the request.
    let response = client
        .request(request)
        .await
        .map_err(|e| http_client_err(e, Some("Unable to send request".into())))?;

    // Deserialize response into correct type.
    let response_body_bytes = response
        .collect()
        .await
        .map_err(|e| http_client_err(e, Some("Unable to read response".into())))?
        .to_bytes();
    let response: RS = serde_json::from_slice(&response_body_bytes)
        .map_err(|e| http_client_err(e, Some("Unable to deserialize response".into())))?;
    Ok(response)
}
