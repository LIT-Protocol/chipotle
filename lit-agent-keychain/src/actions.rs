//! The two pinned Lit Actions this service depends on, plus their CIDs.
//!
//! * `encrypt` — run by the control plane (tenant service key) to seal values.
//! * `reader`  — run by agents directly against Chipotle to read plaintext.
//!
//! The reader has the grant signer's address baked in, so its CID is a
//! function of the deployment's `GRANT_SIGNING_KEY`. Rotating that key changes
//! the CID and requires re-attaching the reader to every tenant group.

use ipfs_hasher::IpfsHasher;

const ENCRYPT_SOURCE: &str = include_str!("../actions/encrypt.js");
const READER_TEMPLATE: &str = include_str!("../actions/reader.js");
const SIGNER_PLACEHOLDER: &str = "__GRANT_SIGNER__";

#[derive(Clone, Debug)]
pub struct ActionSet {
    pub encrypt_code: String,
    pub encrypt_cid: String,
    pub reader_code: String,
    pub reader_cid: String,
    pub grant_signer: String,
}

impl ActionSet {
    pub fn build(grant_signer_address: &str) -> Self {
        let reader_code = READER_TEMPLATE.replace(SIGNER_PLACEHOLDER, grant_signer_address);
        Self {
            encrypt_cid: cid_for_code(ENCRYPT_SOURCE),
            encrypt_code: ENCRYPT_SOURCE.to_string(),
            reader_cid: cid_for_code(&reader_code),
            reader_code,
            grant_signer: grant_signer_address.to_string(),
        }
    }
}

/// IPFS CID (same hasher Chipotle uses to identify inline `code`).
pub fn cid_for_code(code: &str) -> String {
    IpfsHasher::default().compute(code.as_bytes())
}

/// keccak256 of a CID string, 0x-hex — the `hashed_cid` form some Chipotle
/// management endpoints take.
pub fn hashed_cid(cid: &str) -> String {
    format!(
        "0x{}",
        hex::encode(alloy_primitives::keccak256(cid.as_bytes()))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_embeds_signer_and_cid_depends_on_it() {
        let a = ActionSet::build("0x1111111111111111111111111111111111111111");
        let b = ActionSet::build("0x2222222222222222222222222222222222222222");
        assert!(a
            .reader_code
            .contains("0x1111111111111111111111111111111111111111"));
        assert!(!a.reader_code.contains(SIGNER_PLACEHOLDER));
        assert_ne!(a.reader_cid, b.reader_cid);
        assert_eq!(a.encrypt_cid, b.encrypt_cid);
        assert!(a.reader_cid.starts_with("Qm") || a.reader_cid.starts_with("baf"));
    }

    #[test]
    fn hashed_cid_is_hex() {
        let h = hashed_cid("QmTest");
        assert_eq!(h.len(), 66);
        assert!(h.starts_with("0x"));
    }
}
