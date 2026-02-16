use std::path::PathBuf;

use crate::client::hyper::handler::handle_request;
use async_trait::async_trait;
use http_body_util::Full;
use hyper::{Method, Request, body::Bytes, http::request::Builder};
use hyper_util::client::legacy::Client as HyperClient;
use hyperlocal::{UnixClientExt, UnixConnector, Uri};
use serde::{Deserialize, Serialize};

use crate::error::Result;

pub struct UnixClientImpl {
    client: HyperClient<UnixConnector, Full<Bytes>>,
    socket_path: PathBuf,
}

impl UnixClientImpl {
    pub fn new<P>(socket_path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        let client = HyperClient::unix();
        Self { client, socket_path: socket_path.into() }
    }

    pub fn client(&self) -> &HyperClient<UnixConnector, Full<Bytes>> {
        &self.client
    }
}

#[async_trait]
pub trait Client {
    fn post_builder(&self, endpoint_path: &str) -> Builder;
    async fn handle_request<RQ, RS>(&self, builder: Builder, req_body: RQ) -> Result<RS>
    where
        RQ: Serialize + Send + Sync,
        RS: for<'a> Deserialize<'a> + Send;

    // Utility
    async fn post<RQ, RS>(&self, endpoint_path: &str, request: &RQ) -> Result<RS>
    where
        RQ: Serialize + Send + Sync,
        RS: for<'a> Deserialize<'a> + Send,
    {
        self.handle_request(self.post_builder(endpoint_path), request).await
    }
}

#[async_trait]
impl Client for UnixClientImpl {
    fn post_builder(&self, endpoint_path: &str) -> Builder {
        Request::builder()
            .method(Method::POST)
            .uri(Uri::new(self.socket_path.clone(), endpoint_path))
    }

    async fn handle_request<RQ, RS>(&self, builder: Builder, req_body: RQ) -> Result<RS>
    where
        RQ: Serialize + Send,
        RS: for<'a> Deserialize<'a> + Send,
    {
        handle_request(self.client(), builder, req_body).await
    }
}
