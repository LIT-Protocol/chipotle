//! Minimal Alloy bindings for DiamondInit used by the deployer.

use alloy::sol;

sol! {
    function init() external;
}

pub const DIAMONDINIT_JSON: &str = include_str!("./DiamondInit.json");
