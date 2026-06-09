#[cfg(feature = "ipfs")]
use std::path::Path;

#[cfg(feature = "ipfs")]
use tokio::fs;

#[cfg(feature = "ipfs")]
use crate::env::ENV_CACHE_PATH_KEY;
#[cfg(feature = "ipfs")]
use crate::error::Result;
#[cfg(feature = "ipfs")]
use crate::error::unexpected::Unexpected;

#[cfg(feature = "ipfs")]
pub(crate) async fn cache_create_path(cache_path: &Path) -> Result<()> {
    if !fs::try_exists(cache_path).await.unwrap_or(false) {
        fs::create_dir_all(&cache_path).await.expect_or_err(
            format!(
                "failed to create cache dir: {} \
                (hint: you can change the path by setting {})",
                cache_path.to_str().unwrap(),
                ENV_CACHE_PATH_KEY
            )
            .as_str(),
        )?;
    }

    Ok(())
}
