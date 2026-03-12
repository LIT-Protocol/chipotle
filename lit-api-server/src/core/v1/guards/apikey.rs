use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket_okapi::Result as RocketOkapiResult;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::okapi::openapi3::{Object, Parameter, ParameterValue};
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};

/// Request guard that extracts the API key from `Authorization: Bearer <key>` or `X-Api-Key: <key>`.
#[derive(Clone, Debug)]
pub struct ApiKey(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ApiKey {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<ApiKey, Self::Error> {
        let auth = request.headers().get_one("Authorization");
        if let Some(v) = auth {
            let v = v.trim();
            // Parse "Authorization" header in a case-insensitive way for the "Bearer" scheme.
            let mut parts = v.split_whitespace();
            if let (Some(scheme), Some(key_part)) = (parts.next(), parts.next())
                && scheme.eq_ignore_ascii_case("bearer")
            {
                let key = key_part.trim();
                if !key.is_empty() {
                    return Outcome::Success(ApiKey(key.to_string()));
                }
            }
        }
        if let Some(key) = request.headers().get_one("X-Api-Key") {
            let key = key.trim();
            if !key.is_empty() {
                return Outcome::Success(ApiKey(key.to_string()));
            }
        }
        Outcome::Error((Status::Unauthorized, ()))
    }
}

impl<'r> OpenApiFromRequest<'r> for ApiKey {
    fn from_request_input(
        generator: &mut OpenApiGenerator,
        _name: String,
        required: bool,
    ) -> RocketOkapiResult<RequestHeaderInput> {
        let schema = generator.json_schema::<String>();
        Ok(RequestHeaderInput::Parameter(Parameter {
            name: "X-Api-Key".to_owned(),
            location: "header".to_owned(),
            description: Some(
                "Account or usage API key. Alternatively use Authorization: Bearer <key>."
                    .to_owned(),
            ),
            required,
            deprecated: false,
            allow_empty_value: false,
            value: ParameterValue::Schema {
                style: None,
                explode: None,
                allow_reserved: false,
                schema,
                example: None,
                examples: None,
            },
            extensions: Object::default(),
        }))
    }
}
