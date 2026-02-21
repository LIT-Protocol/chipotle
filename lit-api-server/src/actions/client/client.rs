//! This module implements the gRPC client for the Lit Actions server (lit_actions).
//! The client initiates JS code execution and handles op requests from the server.
//! It holds all configuration data (including secrets) and manages state; none of
//! which are shared with lit_actions, enabling a secure execution environment.


use crate::actions::client::{ClientBuilder, DEFAULT_CLIENT_TIMEOUT_MS_BUFFER};
use anyhow::{Result, bail};
use std::path::PathBuf;

use lit_actions_grpc::tonic::metadata::MetadataMap;
use moka::future::Cache;
use tokio::time::Duration;
use super::Client;

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
        Duration::from_millis(self.timeout_ms + DEFAULT_CLIENT_TIMEOUT_MS_BUFFER)
    }

    pub fn request_id(&self) -> String {
        self.request_id.clone().unwrap_or_default()
    }

    pub fn logs(&self) -> &str {
        &self.state.logs
    }

    fn ipfs_cache(&self) -> Result<Cache<String, String>> {
        // if let Some(ipfs_cache) = self.js_env.ipfs_cache.clone() {
        //     return Ok(ipfs_cache);
        // }

        bail!("No IPFS cache found");
    }

    fn http_cache(&self) -> Result<reqwest::Client> {
        if let Some(http_cache) = self.js_env.http_client.clone() {
            return Ok(http_cache);
        }
        bail!("No HTTP cache found");
    }

    pub fn metadata(&self) -> Result<MetadataMap> {
        let mut md = MetadataMap::new();
        // md.insert(
        //     "x-host",
        //     self.lit_config()
        //         .external_addr()
        //         .unwrap_or_default()
        //         .parse()?,
        // );
        md.insert("x-request-id", self.request_id().parse()?);

        // Add trace context to the metadata for distributed tracing.
        // inject_tracing_metadata(&mut md);

        Ok(md)
    }

    pub fn reset_state(&mut self) {
        std::mem::take(&mut self.state);
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
