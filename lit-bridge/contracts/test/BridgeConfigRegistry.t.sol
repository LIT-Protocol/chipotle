// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {BridgeConfigRegistry} from "../src/BridgeConfigRegistry.sol";

contract BridgeConfigRegistryTest is Test {
    BridgeConfigRegistry registry;

    address owner = address(0xA11CE);
    address safe = address(0x5AFE);
    address stranger = address(0xBAD);

    function setUp() public {
        vm.prank(owner);
        registry = new BridgeConfigRegistry(owner);
    }

    function _alchemy(string memory secret)
        internal
        pure
        returns (BridgeConfigRegistry.RpcEntry memory)
    {
        return BridgeConfigRegistry.RpcEntry({
            rpcType: BridgeConfigRegistry.RpcType.Alchemy,
            host: "",
            encSecret: secret
        });
    }

    function _custom(string memory host, string memory secret)
        internal
        pure
        returns (BridgeConfigRegistry.RpcEntry memory)
    {
        return BridgeConfigRegistry.RpcEntry({
            rpcType: BridgeConfigRegistry.RpcType.Custom,
            host: host,
            encSecret: secret
        });
    }

    function test_storesAndReadsBackConfig() public {
        BridgeConfigRegistry.RpcEntry[] memory rpcs = new BridgeConfigRegistry.RpcEntry[](2);
        rpcs[0] = _alchemy("cipher-alchemy");
        rpcs[1] = BridgeConfigRegistry.RpcEntry({
            rpcType: BridgeConfigRegistry.RpcType.Infura,
            host: "",
            encSecret: "cipher-infura"
        });

        vm.prank(owner);
        registry.setChain(84532, 5, 2, rpcs);

        (bool exists, uint64 minConf, uint8 quorum, uint256 rpcCount) = registry.getChain(84532);
        assertTrue(exists);
        assertEq(minConf, 5);
        assertEq(quorum, 2);
        assertEq(rpcCount, 2);

        (uint8 rpcType,, string memory encSecret) = registry.getRpc(84532, 0);
        assertEq(rpcType, uint8(BridgeConfigRegistry.RpcType.Alchemy));
        assertEq(encSecret, "cipher-alchemy");
    }

    function test_revertsOnZeroQuorum() public {
        BridgeConfigRegistry.RpcEntry[] memory rpcs = new BridgeConfigRegistry.RpcEntry[](1);
        rpcs[0] = _alchemy("c");
        vm.prank(owner);
        vm.expectRevert(BridgeConfigRegistry.ZeroQuorum.selector);
        registry.setChain(1, 5, 0, rpcs);
    }

    function test_revertsWhenQuorumExceedsRpcCount() public {
        BridgeConfigRegistry.RpcEntry[] memory rpcs = new BridgeConfigRegistry.RpcEntry[](1);
        rpcs[0] = _alchemy("c");
        vm.prank(owner);
        vm.expectRevert(BridgeConfigRegistry.QuorumExceedsRpcCount.selector);
        registry.setChain(1, 5, 2, rpcs);
    }

    function test_requiresCustomHostAndNonEmptySecret() public {
        BridgeConfigRegistry.RpcEntry[] memory bad = new BridgeConfigRegistry.RpcEntry[](1);
        bad[0] = _custom("", "c");
        vm.prank(owner);
        vm.expectRevert(BridgeConfigRegistry.CustomHostRequired.selector);
        registry.setChain(1, 5, 1, bad);

        BridgeConfigRegistry.RpcEntry[] memory empty = new BridgeConfigRegistry.RpcEntry[](1);
        empty[0] = _alchemy("");
        vm.prank(owner);
        vm.expectRevert(BridgeConfigRegistry.EmptySecret.selector);
        registry.setChain(1, 5, 1, empty);
    }

    function test_onlyOwnerWritesAndTwoStepSafeHandoff() public {
        BridgeConfigRegistry.RpcEntry[] memory rpcs = new BridgeConfigRegistry.RpcEntry[](1);
        rpcs[0] = _alchemy("c");

        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, stranger)
        );
        registry.setChain(1, 5, 1, rpcs);

        // Two-step transfer to the Safe.
        vm.prank(owner);
        registry.transferOwnership(safe);
        assertEq(registry.owner(), owner); // not yet

        vm.prank(safe);
        registry.acceptOwnership();
        assertEq(registry.owner(), safe);
    }

    function test_replaceWholesaleAndRemove() public {
        BridgeConfigRegistry.RpcEntry[] memory one = new BridgeConfigRegistry.RpcEntry[](1);
        one[0] = _alchemy("a");
        vm.prank(owner);
        registry.setChain(1, 5, 1, one);

        BridgeConfigRegistry.RpcEntry[] memory two = new BridgeConfigRegistry.RpcEntry[](2);
        two[0] = _alchemy("a2");
        two[1] = _custom("rpc.x.io", "u");
        vm.prank(owner);
        registry.setChain(1, 9, 2, two);

        (, uint64 minConf, uint8 quorum, uint256 rpcCount) = registry.getChain(1);
        assertEq(minConf, 9);
        assertEq(quorum, 2);
        assertEq(rpcCount, 2);

        vm.prank(owner);
        registry.removeChain(1);
        (bool exists,,,) = registry.getChain(1);
        assertFalse(exists);

        vm.expectRevert(BridgeConfigRegistry.ChainNotConfigured.selector);
        registry.getRpc(1, 0);
    }
}
