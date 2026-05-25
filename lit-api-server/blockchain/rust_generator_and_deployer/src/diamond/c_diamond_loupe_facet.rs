//! Minimal Alloy bindings for DiamondLoupeFacet used by the deployer.

use alloy::sol;

sol! {
    function facetAddresses() external view returns (address[] memory);
    function facetFunctionSelectors(address _facet) external view returns (bytes4[] memory);
}

pub const DIAMONDLOUPEFACET_JSON: &str = include_str!("./DiamondLoupeFacet.json");
