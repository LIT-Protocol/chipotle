//! This module implements the gRPC client for the Lit Actions server (lit_actions).
//! The client initiates JS code execution and handles op requests from the server.
//! It holds all configuration data (including secrets) and manages state; none of
//! which are shared with lit_actions, enabling a secure execution environment.

use crate::actions::client::ClientBuilder;
use crate::core::v1::models::response::LitActionClientConfigResponse;
use anyhow::{Result, bail};
use std::path::PathBuf;

use super::Client;
use lit_actions_grpc::tonic::metadata::MetadataMap;
use moka::future::Cache;
use tokio::time::Duration;

impl Client {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        ClientBuilder::default()
            .socket_path(socket_path)
            .build()
            .expect("cannot fail")
    }

    pub fn socket_path(&self) -> PathBuf {
        self.socket_path.clone().unwrap_or_default()
    }

    pub fn client_timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms + self.client_timeout_ms_buffer)
    }

    pub fn request_id(&self) -> String {
        self.request_id.clone().unwrap_or_default()
    }

    pub fn logs(&self) -> &str {
        &self.state.logs
    }

    #[allow(dead_code)]
    pub fn ipfs_cache(&self) -> Result<Cache<String, String>> {
        // if let Some(ipfs_cache) = self.js_env.ipfs_cache.clone() {
        //     return Ok(ipfs_cache);
        // }

        bail!("No IPFS cache found");
    }

    #[allow(dead_code)]
    pub fn http_cache(&self) -> Result<reqwest::Client> {
        if let Some(http_cache) = self.js_env.http_client.clone() {
            return Ok(http_cache);
        }
        bail!("No HTTP cache found");
    }

    pub fn metadata(&self) -> Result<MetadataMap> {
        let mut md = MetadataMap::new();
        md.insert("x-request-id", self.request_id().parse()?);

        // TODO: x-correlation-id is also sent via ExecutionRequest.http_headers —
        // this redundancy in gRPC metadata could be simplified later.
        if let Some(cid) = self.http_headers.get("x-correlation-id") {
            md.insert("x-correlation-id", cid.parse()?);
        }

        // Inject W3C TraceContext headers into gRPC metadata for distributed tracing.
        // This allows TracingMiddleware on the lit-actions side to reconstruct the parent span.
        #[cfg(feature = "otlp")]
        {
            use lit_observability::opentelemetry::propagation::TextMapPropagator;
            use lit_observability::opentelemetry_sdk::propagation::TraceContextPropagator;
            use lit_observability::tracing_opentelemetry::OpenTelemetrySpanExt;

            let cx = tracing::Span::current().context();
            let propagator = TraceContextPropagator::new();
            let mut injector = lit_observability::tracing::propagation::TonicMetadataMap(&mut md);
            propagator.inject_context(&cx, &mut injector);
        }

        Ok(md)
    }

    pub fn reset_state(&mut self) {
        std::mem::take(&mut self.state);
    }

    /// Returns a snapshot of the effective configuration values for this client instance.
    pub fn config_snapshot(&self) -> LitActionClientConfigResponse {
        LitActionClientConfigResponse {
            timeout_ms: self.timeout_ms,
            async_timeout_ms: self.async_timeout_ms,
            memory_limit_mb: self.memory_limit_mb,
            max_code_length: self.max_code_length,
            max_response_length: self.max_response_length,
            max_console_log_length: self.max_console_log_length,
            max_fetch_count: self.max_fetch_count,
            max_get_keys_count: self.max_get_keys_count,
            max_retries: self.max_retries,
            client_timeout_ms_buffer: self.client_timeout_ms_buffer,
        }
    }

    // async fn pay(&mut self, price_component: LitActionPriceComponent, price: u64) -> Result<()> {
    //     if let Err(e) = self.dynamic_payment.add(price_component, price) {
    //         bail!(e);
    //     }
    //     Ok(())
    // }

    // fn increment_broad_and_collect_counter(&mut self) -> Result<()> {
    //     self.state.broadcast_and_collect_count += 1;
    //     if self.state.broadcast_and_collect_count > self.max_broadcast_and_collect_count {
    //         bail!(
    //             "You may not use broadcast and collect functionality more than {} times per session and you have attempted to exceed that limit.",
    //             self.max_broadcast_and_collect_count,
    //         );
    //     };
    //     Ok(())
    // }
}
