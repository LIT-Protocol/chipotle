use rocket::{http::Status, response::status::Custom, serde::json::Json};
use serde::Serialize;
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}
pub type ApiError = Custom<Json<ErrorResponse>>;
pub type ApiResult<T> = Result<Json<T>, ApiError>;
pub fn err(status: Status, error: &str) -> ApiError {
    Custom(
        status,
        Json(ErrorResponse {
            error: error.into(),
        }),
    )
}
pub fn invalid(_: impl std::fmt::Display) -> ApiError {
    err(Status::BadRequest, "invalid_request")
}
pub fn denied(_: impl std::fmt::Display) -> ApiError {
    err(Status::Forbidden, "authorization_denied")
}
pub fn internal(e: impl std::fmt::Display) -> ApiError {
    // Never include query parameters, proof material, action logs or upstream bodies.
    let _ = e;
    tracing::warn!("keychain storage operation failed");
    err(Status::InternalServerError, "storage_unavailable")
}

/// Rocket's default JSON data error includes the raw request body in its Debug
/// output. Secret-bearing endpoints must discard that error before route-level
/// logging, including malformed JSON and schema errors.
pub struct PrivateJson<T>(T);
impl<T> PrivateJson<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}
impl<T> std::ops::Deref for PrivateJson<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
#[rocket::async_trait]
impl<'r, T: serde::Deserialize<'r> + Send> rocket::data::FromData<'r> for PrivateJson<T> {
    type Error = ();
    async fn from_data(
        req: &'r rocket::Request<'_>,
        data: rocket::Data<'r>,
    ) -> rocket::data::Outcome<'r, Self> {
        use rocket::data::{FromData, Outcome};
        match <Json<T> as FromData>::from_data(req, data).await {
            Outcome::Success(value) => Outcome::Success(Self(value.into_inner())),
            Outcome::Error((status, _)) => Outcome::Error((status, ())),
            Outcome::Forward(value) => Outcome::Forward(value),
        }
    }
}

#[cfg(test)]
mod private_json_tests {
    use super::*;
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        code: String,
    }
    #[rocket::post("/", data = "<body>")]
    fn parse(body: Result<PrivateJson<Input>, ()>) -> String {
        match body {
            Ok(value) => value.code.clone(),
            Err(error) => format!("{error:?}"),
        }
    }
    #[rocket::async_test]
    async fn malformed_secret_bodies_have_only_a_redacted_guard_error() {
        let client = rocket::local::asynchronous::Client::tracked(
            rocket::build().mount("/", rocket::routes![parse]),
        )
        .await
        .unwrap();
        for body in [
            r#"{"code":"sensitive","extra":"private"}"#,
            r#"{"code":"sensitive""#,
            r#"{"secret":"sensitive"}"#,
        ] {
            let response = client
                .post("/")
                .header(rocket::http::ContentType::JSON)
                .body(body)
                .dispatch()
                .await;
            assert_eq!(response.into_string().await.unwrap(), "()");
        }
    }
}
