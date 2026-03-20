use ahash::{HashSet, HashSetExt};
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

        metrics::counter!(
            "service.request",
            "http.method" => req.method().as_str().to_owned(),
            "url.path" => req.uri().path().to_string(),
            "sdk.version" => sdk_version.to_owned(),
        )
        .increment(1);
    }

    async fn on_response<'r>(&self, req: &'r Request<'_>, res: &mut Response<'r>) {
        // Do not track metrics for invalid routes.
        if !self.valid_routes.contains(format!("{} {}", req.method(), req.uri().path()).as_str()) {
            return;
        }

        metrics::counter!(
            "service.response",
            "http.method" => req.method().as_str().to_owned(),
            "http.status_code" => res.status().to_string(),
        )
        .increment(1);
    }
}
