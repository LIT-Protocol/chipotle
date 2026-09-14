use crate::{
    crypto,
    models::{Authority, Manifest},
};
use anyhow::Result;
use ipfs_hasher::IpfsHasher;
use serde::Serialize;
const AUTHORITY: &str = include_str!("../generated/authority.js");
const EXPORT: &str = include_str!("../generated/export.js");
const STRIPE: &str = include_str!("../generated/stripe-balance.js");
pub const PUBLIC_KEY: &str = include_str!("../actions/public-key.js");
pub fn source<T: Serialize>(base: &str, manifest: &T) -> Result<String> {
    Ok(format!("{base}\nconst KEYCHAIN_MANIFEST={};\nasync function main(params){{return KeychainAction.run(KEYCHAIN_MANIFEST,params)}}\n", crypto::canonical(&serde_json::to_value(manifest)?)?))
}
pub fn authority_source(a: &Authority) -> Result<String> {
    source(AUTHORITY, a)
}
pub fn secret_source(m: &Manifest) -> Result<String> {
    source(
        if m.release == "export" {
            EXPORT
        } else {
            STRIPE
        },
        m,
    )
}
pub fn cid(code: &str) -> String {
    IpfsHasher::default().compute(code.as_bytes())
}
