// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {BaseTest} from "./helpers/BaseTest.sol";
import {AppStorage} from "../contracts/AccountConfigFacets/AppStorage.sol";
import {ViewsFacet} from "../contracts/AccountConfigFacets/ViewsFacet.sol";
import {WritesFacet} from "../contracts/AccountConfigFacets/WritesFacet.sol";
import {EnumerableSet} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {IDiamond} from "../interfaces/IDiamond.sol";

/// @dev Read-only test facet: eviction makes the old usage slot unreachable via
/// public resolution, so inspect its membership without rewriting any storage.
contract UsageKeyDeletionProbe {
    using EnumerableSet for EnumerableSet.UintSet;

    function permissionContains(uint256 master, uint256 key, uint256 field, uint256 group)
        external
        view
        returns (bool)
    {
        AppStorage.UsageApiKey storage k =
            AppStorage.getStorage().accounts[master].usageApiKeys[key];
        if (field == 0) return k.manageIPFSIdsInGroups.contains(group);
        if (field == 1) return k.addPkpToGroups.contains(group);
        if (field == 2) return k.removePkpFromGroups.contains(group);
        return k.executeInGroups.contains(group);
    }
}

/// @notice Independent regression coverage through the production diamond.
contract UsageKeyDeletionTest is BaseTest {
    uint256 constant MASTER = 101;
    uint256 constant USAGE = 202;
    address constant WALLET = address(0x1234);
    address constant NEW_WALLET = address(0x5678);
    uint256 first;
    uint256 second;
    UsageKeyDeletionProbe probe;

    function setUp() public override {
        super.setUp();
        UsageKeyDeletionProbe implementation = new UsageKeyDeletionProbe();
        bytes4[] memory selectors = new bytes4[](1);
        selectors[0] = UsageKeyDeletionProbe.permissionContains.selector;
        IDiamond.FacetCut[] memory changes = new IDiamond.FacetCut[](1);
        changes[0] =
            IDiamond.FacetCut(address(implementation), IDiamond.FacetCutAction.Add, selectors);
        vm.prank(owner);
        cut.diamondCut(changes, address(0), "");
        probe = UsageKeyDeletionProbe(d.diamond);
        vm.startPrank(apiPayer);
        writes.newAccount(MASTER, true, "account", "", user);
        address[] memory wallets = new address[](1);
        wallets[0] = WALLET;
        first = writes.addGroup(MASTER, "first", "", _one(301), wallets);
        second = writes.addGroup(MASTER, "second", "", _one(302), wallets);
        vm.stopPrank();
    }

    function _one(uint256 value) internal pure returns (uint256[] memory a) {
        a = new uint256[](1);
        a[0] = value;
    }

    function _original(bool wildcard) internal view returns (uint256[] memory a) {
        a = new uint256[](3);
        a[0] = first;
        a[1] = second;
        a[2] = wildcard ? 0 : 999;
    }

    function _set(uint256 key, uint256[] memory scopes) internal {
        vm.prank(apiPayer);
        writes.setUsageApiKey(
            MASTER, key, 0, 0, "usage", "", false, false, false, scopes, scopes, scopes, scopes
        );
    }

    function _remove() internal {
        vm.prank(apiPayer);
        writes.removeUsageApiKey(MASTER, USAGE);
    }

    function _listed(uint256[] memory expected) internal view {
        ViewsFacet.UsageApiKeyReturn[] memory keys = views_.listApiKeys(MASTER, 0, 100);
        assertEq(keys.length, 1);
        assertEq(keys[0].apiKeyHash, USAGE);
        assertEq(keys[0].manageIPFSIdsInGroups, expected, "listed CID scope");
        assertEq(keys[0].addPkpToGroups, expected, "listed add scope");
        assertEq(keys[0].removePkpFromGroups, expected, "listed remove scope");
        assertEq(keys[0].executeInGroups, expected, "listed execution scope");
    }

    function _execution(uint256 cid, bool expected) internal view {
        assertEq(views_.canExecuteAction(USAGE, cid), expected, "execution");
        assertEq(views_.canExecuteActionFast(USAGE, cid), expected, "fast execution");
        assertEq(views_.canUseWalletInAction(USAGE, cid, WALLET), expected, "wallet");
        assertEq(views_.canUseWalletInActionFast(USAGE, cid, WALLET), expected, "fast wallet");
        (bool execute, bool wallet) = views_.canExecuteActionAndUseWallet(USAGE, cid, WALLET);
        assertEq(execute, expected, "combined execution");
        assertEq(wallet, expected, "combined wallet");
    }

    // Real writes exercise the three SecurityLib membership guards. Separate
    // snapshots keep one successful control from affecting the next operation.
    function _management(uint256 group, bool expected) internal {
        for (uint256 route; route < 4; ++route) {
            uint256 snapshot = vm.snapshotState();
            bytes memory callData;
            bytes memory denied;
            if (route == 0) {
                callData = abi.encodeCall(WritesFacet.addPkpToGroup, (USAGE, group, NEW_WALLET));
                denied = abi.encodeWithSelector(
                    AppStorage.NotAllowedToAddPkpToGroup.selector, USAGE, group
                );
            } else if (route == 1) {
                callData = abi.encodeCall(WritesFacet.removePkpFromGroup, (USAGE, group, WALLET));
                denied = abi.encodeWithSelector(
                    AppStorage.NotAllowedToRemovePkpFromGroup.selector, USAGE, group
                );
            } else if (route == 2) {
                callData = abi.encodeCall(WritesFacet.addActionToGroup, (USAGE, group, 777));
                denied = abi.encodeWithSelector(
                    AppStorage.NotAllowedToManageIPFSIdsInGroup.selector, USAGE, group
                );
            } else {
                uint256 cid = group == first ? 301 : 302;
                callData = abi.encodeCall(WritesFacet.removeActionFromGroup, (USAGE, group, cid));
                denied = abi.encodeWithSelector(
                    AppStorage.NotAllowedToManageIPFSIdsInGroup.selector, USAGE, group
                );
            }
            bytes32 beforeState = keccak256(abi.encode(views_.listGroupContents(MASTER, group)));
            vm.prank(apiPayer);
            (bool ok, bytes memory result) = address(writes).call(callData);
            assertEq(ok, expected, "management authorization disagrees with listed scope");
            bytes32 afterState = keccak256(abi.encode(views_.listGroupContents(MASTER, group)));
            if (expected) {
                assertNotEq(afterState, beforeState, "positive control did not mutate");
            } else {
                assertEq(result, denied);
                assertEq(afterState, beforeState, "denial changed state");
            }
            assertTrue(vm.revertToState(snapshot));
        }
    }

    function test_recreateEmptyRevokesExecution() public {
        _set(USAGE, _original(false));
        _execution(301, true);
        _remove();
        _set(USAGE, new uint256[](0));
        _listed(new uint256[](0));
        _execution(301, false);
        _execution(302, false);
    }

    function test_recreateEmptyRevokesManagement() public {
        _set(USAGE, _original(false));
        _management(first, true);
        _remove();
        _set(USAGE, new uint256[](0));
        _listed(new uint256[](0));
        _management(first, false);
        _management(second, false);
    }

    function test_recreateReducedScopesKeepOnlyReplacement() public {
        _set(USAGE, _original(false));
        _remove();
        // Overlaps an old member: stale positions can also prevent add() from
        // restoring it to the enumerable list, even though contains() is true.
        _set(USAGE, _one(second));
        _listed(_one(second));
        _execution(301, false);
        _management(first, false);
        _execution(302, true);
        _management(second, true);
    }

    function test_recreateFreshReplacementRemainsUsable() public {
        _set(USAGE, _one(first));
        _remove();
        _set(USAGE, _one(second));
        _listed(_one(second));
        _execution(302, true);
        _management(second, true);
        _execution(301, false);
        _management(first, false);
    }

    function test_recreateDoesNotRetainWildcardZero() public {
        _set(USAGE, _one(0));
        assertTrue(views_.canExecuteAction(USAGE, 987654));
        _execution(301, true);
        _remove();
        _set(USAGE, _one(second));
        _listed(_one(second));
        assertFalse(views_.canExecuteAction(USAGE, 987654), "stale wildcard");
        _execution(301, false);
        _execution(302, true);
    }

    function test_directUpdateAlreadyClearsAllScopes() public {
        _set(USAGE, _original(true));
        _set(USAGE, _one(second));
        _listed(_one(second));
        assertFalse(views_.canExecuteAction(USAGE, 987654));
        _execution(301, false);
        _management(first, false);
        _execution(302, true);
        _management(second, true);
        _set(USAGE, new uint256[](0));
        _listed(new uint256[](0));
        _execution(302, false);
        _management(second, false);
    }

    function test_removalWithoutRecreationAlreadyRevokes() public {
        _set(USAGE, _original(true));
        _remove();
        assertEq(views_.listApiKeys(MASTER, 0, 100).length, 0);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.AccountDoesNotExist.selector, USAGE));
        views_.canExecuteAction(USAGE, 301);
        vm.prank(apiPayer);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.NoAccountAccess.selector, USAGE, apiPayer)
        );
        writes.addPkpToGroup(USAGE, first, NEW_WALLET);
    }

    function testFuzz_removalClearsEachPermissionMapping(uint8 fieldSeed) public {
        uint256 field = bound(fieldSeed, 0, 3);
        _set(USAGE, _original(true));
        assertTrue(probe.permissionContains(MASTER, USAGE, field, first));
        assertTrue(probe.permissionContains(MASTER, USAGE, field, 0));
        _remove();
        assertFalse(
            probe.permissionContains(MASTER, USAGE, field, first), "orphaned group position"
        );
        assertFalse(
            probe.permissionContains(MASTER, USAGE, field, second), "orphaned second position"
        );
        assertFalse(probe.permissionContains(MASTER, USAGE, field, 0), "orphaned zero position");
    }

    function testFuzz_squatEvictionClearsEachPermissionMapping(uint8 fieldSeed) public {
        uint256 field = bound(fieldSeed, 0, 3);
        uint256 victim = apiKeyHashOf(stranger);
        _set(victim, _original(true));
        assertTrue(probe.permissionContains(MASTER, victim, field, first));
        assertTrue(probe.permissionContains(MASTER, victim, field, 0));
        vm.prank(stranger);
        writes.newChainSecuredAccount("victim", "");
        assertEq(views_.listApiKeys(MASTER, 0, 100).length, 0);
        assertEq(views_.getAccountWalletAddress(victim), stranger);
        assertFalse(
            probe.permissionContains(MASTER, victim, field, first),
            "eviction orphaned group position"
        );
        assertFalse(
            probe.permissionContains(MASTER, victim, field, second),
            "eviction orphaned second position"
        );
        assertFalse(
            probe.permissionContains(MASTER, victim, field, 0), "eviction orphaned zero position"
        );
        // The old account cannot reclaim the now-sovereign master hash.
        vm.expectRevert(abi.encodeWithSelector(AppStorage.AccountAlreadyExists.selector, victim));
        _set(victim, new uint256[](0));
    }

    function test_removedGroupIdsAreNotReused() public {
        vm.prank(apiPayer);
        writes.removeGroup(MASTER, first);
        vm.prank(apiPayer);
        uint256 next = writes.addGroup(MASTER, "next", "", _one(303), new address[](0));
        assertGt(next, second, "deleted group slot reused");
        assertEq(views_.groupIdsForAction(MASTER, 301).length, 0);
    }
}
