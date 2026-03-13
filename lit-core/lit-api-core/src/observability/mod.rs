use ahash::{HashSet, HashSetExt};
use lit_observability::metrics::counter;
use opentelemetry::KeyValue;
use opentelemetry_semantic_conventions::resource::{HTTP_METHOD, HTTP_STATUS_CODE, URL_PATH};
use rocket::{Data, Request, Response, Route, fairing::Fairing};

use crate::context::HEADER_KEY_X_LIT_SDK_VERSION;

pub struct MetricsFairings {
    valid_routes: HashSet<String>,
}

impl MetricsFairings {
    pub fn new(valid_routes: Vec<Route>) -> Self {
        // Loop through each Route, hash the method and path, and add to a HashSet.
        let mut valid_routes_set: HashSet<String> = HashSet::with_capacity(valid_routes.len());
        for route in valid_routes {
            let method = route.method.as_str();
            let path = route.uri.path().to_string();
            valid_routes_set.insert(format!("{} {}", method, path));
        }

        Self { valid_routes: valid_routes_set }
    }
}

#[rocket::async_trait]
impl Fairing for MetricsFairings {
    fn info(&self) -> rocket::fairing::Info {
        rocket::fairing::Info {
            name: "Metrics Fairing",
            kind: rocket::fairing::Kind::Request | rocket::fairing::Kind::Response,
        }
    }

    async fn on_request(&self, req: &mut Request<'_>, _: &mut Data<'_>) {
        // Do not track metrics for invalid routes.
        if !self.valid_routes.contains(format!("{} {}", req.method(), req.uri().path()).as_str()) {
            return;
        }

        // Get the SDK-Version header value.
        let sdk_version = req.headers().get_one(HEADER_KEY_X_LIT_SDK_VERSION).unwrap_or("unknown");

        counter::add_one(
            http::HttpMetrics::ServiceRequest,
            &[
                KeyValue::new(HTTP_METHOD, req.method().as_str()),
                KeyValue::new(URL_PATH, req.uri().path().to_string()),
                KeyValue::new("sdk.version", sdk_version.to_owned()),
            ],
        );
    }

    async fn on_response<'r>(&self, req: &'r Request<'_>, res: &mut Response<'r>) {
        // Do not track metrics for invalid routes.
        if !self.valid_routes.contains(format!("{} {}", req.method(), req.uri().path()).as_str()) {
            return;
        }

        counter::add_one(
            http::HttpMetrics::ServiceResponse,
            &[
                KeyValue::new(HTTP_METHOD, req.method().as_str()),
                KeyValue::new(HTTP_STATUS_CODE, res.status().to_string()),
            ],
        );
    }
}

pub mod http {
    use lit_observability::metrics::LitMetric;

    pub enum HttpMetrics {
        ServiceRequest,
        PrivacyModeRequest,
        ServiceResponse,
    }

    impl LitMetric for HttpMetrics {
        fn get_meter(&self) -> &str {
            "lit.http"
        }
        fn get_description(&self) -> &str {
            "Counter for HTTP requests and responses."
        }
        fn get_unit(&self) -> &str {
            ""
        }
        fn get_namespace(&self) -> &str {
            "service"
        }
        fn get_name(&self) -> &str {
            match self {
                Self::ServiceRequest => "request",
                Self::PrivacyModeRequest => "request.privacy_mode",
                Self::ServiceResponse => "response",
            }
        }
    }
}
