// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script, console} from "forge-std/Script.sol";
import {BridgeConfigRegistry} from "../src/BridgeConfigRegistry.sol";
import {BridgeToken} from "../src/BridgeToken.sol";

/// @notice Deploy the BridgeConfigRegistry (control plane) to Base.
///
/// Usage:
///   forge script script/Deploy.s.sol:DeployRegistry \
///     --rpc-url $BASE_SEPOLIA_RPC_URL --broadcast \
///     --private-key $DEPLOYER_PRIVATE_KEY
///
/// `REGISTRY_OWNER` should be the Base Safe in production. For local dev it can
/// be the deployer EOA; transfer ownership to the Safe later via
/// transferOwnership + acceptOwnership (two-step).
contract DeployRegistry is Script {
    function run() external returns (BridgeConfigRegistry registry) {
        address owner = vm.envOr("REGISTRY_OWNER", msg.sender);
        vm.startBroadcast();
        registry = new BridgeConfigRegistry(owner);
        vm.stopBroadcast();
        console.log("BridgeConfigRegistry:", address(registry));
        console.log("owner:", owner);
    }
}

/// @notice Deploy a BridgeToken on one chain, pinning the bridge oracle address.
///
/// Usage:
///   ORACLE_ADDRESS=0x... TOKEN_NAME="Bridge Coin" TOKEN_SYMBOL=BRDG \
///   INITIAL_SUPPLY=1000000000000000000000000 \
///   forge script script/Deploy.s.sol:DeployToken \
///     --rpc-url $RPC_URL --broadcast --private-key $DEPLOYER_PRIVATE_KEY
///
/// `ORACLE_ADDRESS` is the bridge signing account's address (Option B) — stable
/// across action upgrades. Mint initial supply only on the home chain.
contract DeployToken is Script {
    function run() external returns (BridgeToken token) {
        address oracle = vm.envAddress("ORACLE_ADDRESS");
        string memory name = vm.envOr("TOKEN_NAME", string("Bridge Coin"));
        string memory symbol = vm.envOr("TOKEN_SYMBOL", string("BRDG"));
        uint256 initialSupply = vm.envOr("INITIAL_SUPPLY", uint256(0));

        vm.startBroadcast();
        token = new BridgeToken(name, symbol, initialSupply, oracle);
        vm.stopBroadcast();
        console.log("BridgeToken:", address(token));
        console.log("oracle:", oracle);
        console.log("initialSupply:", initialSupply);
    }
}
