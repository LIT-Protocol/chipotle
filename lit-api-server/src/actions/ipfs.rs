use ipfs_hasher::IpfsHasher;
use lit_core::config::LitConfig;
use moka::future::Cache;
use std::sync::Arc;
use tracing::{debug, instrument};
use tracing::debug_span;

use crate::error::{ipfs_err, conversion_err, unexpected_err};

#[instrument(level = "debug", skip(ipfs_cache))]
pub async fn get_ipfs_file(
    ipfs_id: &String,
    cfg: &LitConfig,
    ipfs_cache: Cache<String, Arc<String>>,
    http_cache: reqwest::Client,
) -> Result<Arc<String>> {
    // check the cache first
    if let Some(cached_file) = ipfs_cache.get(ipfs_id).await {
        // cache hit
        return Ok(cached_file);
    }
    // cache miss
    let text_result = Arc::new(retrieve_from_ipfs(ipfs_id, cfg, http_cache).await?);
    ipfs_cache
        .insert(ipfs_id.clone(), text_result.clone())
        .await;
    Ok(text_result)
}

async fn retrieve_from_ipfs(
    ipfs_id: &String,
    cfg: &LitConfig,
    http_client: reqwest::Client,
) -> Result<String> {
    let default_entry = RpcEntry::new(RpcKind::IPFS, cfg.ipfs_gateway(), None, None);

    let rpc_resolver_struct = &RPC_RESOLVER;
    let rpc_resolver = rpc_resolver_struct.load_full();
    let mut ipfs_gateways = rpc_resolver
        .resolve("ipfs_gateways")
        .unwrap_or(&vec![])
        .clone();

    if ipfs_gateways.is_empty() {
        ipfs_gateways.push(default_entry);
    }

    let gateway = ipfs_gateways
        .first()
        .expect("ipfs_gateways should always have one entry");

    let start_time = SystemTime::now();
    // TODO: set a max filesize for retrieval
    // TODO: use apikey & headers
    let url = gateway.url().replace("{}", ipfs_id.as_str());
    let req = http_client.get(&url).send().instrument(debug_span!("fetch_ipfs_file")).await
        .map_err(|e| {
            if e.is_timeout() {
                ipfs_err(e, Some("Timeout error getting code from ipfs".into()))
                    .add_detail(format!("Timeout error getting code from ipfs. Try getting it yourself in a browser and see if it works: {url}"))
            } else {
                ipfs_err(e, Some("Error getting ipfs file".into()))
                    .add_detail(format!("Error getting ipfs file: {ipfs_id}"))
            }
        })?;

    if req.status() != 200 {
        return Err(ipfs_err(
            format!(
                "Error getting code from ipfs url. Status code: {}.  Url: {url}",
                req.status()
            ),
            None,
        )
        .add_detail(format!("Error getting ipfs file: {ipfs_id}")));
    }
    let text_result = req.text().await.map_err(|e| {
        conversion_err(
            e,
            Some("Failed to get text from response during IPFS fetch".into()),
        )
    })?;

    if text_result.len() > 30000000 {
        return Err(ipfs_err(
            format!(
                "Error getting code from ipfs url. File too large: {}",
                text_result.len()
            ),
            None,
        ));
    }

    // verify the hash
    let ipfs_hasher = IpfsHasher::default();
    let cid = ipfs_hasher.compute(text_result.as_bytes());
    if cid != ipfs_id.clone() {
        return Err(ipfs_err(
            format!(
                "Error getting code from ipfs url.  Hash mismatch.  Expected: {ipfs_id}  Actual: {cid}"
            ),
            None,
        ));
    }

    let end_time = SystemTime::now();
    let elapsed = end_time
        .duration_since(start_time)
        .map_err(|e| unexpected_err(e, Some("Unable to get duration".into())))?;
    debug!("Retrieved from IPFS in {}ms", elapsed.as_millis());

    Ok(text_result)
}