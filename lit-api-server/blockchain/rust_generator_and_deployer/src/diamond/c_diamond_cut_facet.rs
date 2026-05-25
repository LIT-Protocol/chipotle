//! Minimal Alloy bindings for DiamondCutFacet used by the deployer.

use alloy::sol;

sol! {
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct FacetCut {
        address facetAddress;
        uint8 action;
        bytes4[] functionSelectors;
    }

    function diamondCut(FacetCut[] _diamondCut, address _init, bytes _calldata) external;
}

pub const DIAMONDCUTFACET_JSON: &str = include_str!("./DiamondCutFacet.json");
