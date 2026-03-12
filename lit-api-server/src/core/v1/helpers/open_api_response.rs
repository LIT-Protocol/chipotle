use rocket::response::Responder;
use rocket_okapi::OpenApiError;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::response::OpenApiResponderInner;
use rocket_responder::ApiResponse;
use schemars::JsonSchema;
use serde::Serialize;

pub struct OpenApiResponse<T: Serialize + JsonSchema, E: Serialize + JsonSchema> {
    pub response: ApiResponse<T, E>,
}

impl<T: Serialize + JsonSchema, E: Serialize + JsonSchema> OpenApiResponderInner
    for OpenApiResponse<T, E>
{
    fn responses(
        generator: &mut OpenApiGenerator,
    ) -> std::result::Result<rocket_okapi::okapi::openapi3::Responses, OpenApiError> {
        let mut responses = rocket_okapi::okapi::openapi3::Responses::default();
        let success_schema = generator.json_schema::<T>();
        let error_schema = generator.json_schema::<E>();

        let mut combined_schema = rocket_okapi::okapi::openapi3::SchemaObject::default();
        combined_schema.subschemas = Some(Box::new(schemars::schema::SubschemaValidation {
            one_of: Some(vec![
                schemars::schema::Schema::Object(success_schema),
                schemars::schema::Schema::Object(error_schema),
            ]),
            ..Default::default()
        }));

        rocket_okapi::util::add_default_response_schema(
            &mut responses,
            "application/json".to_string(),
            combined_schema,
        );
        Ok(responses)
    }
}

impl<'r, T: Serialize + JsonSchema, E: Serialize + JsonSchema> Responder<'r, 'static>
    for OpenApiResponse<T, E>
{
    fn respond_to(self, request: &'r rocket::Request<'_>) -> rocket::response::Result<'static> {
        self.response.respond_to(request)
    }
}
