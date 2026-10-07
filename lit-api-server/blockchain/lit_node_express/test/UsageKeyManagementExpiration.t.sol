// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {BaseTest} from "./helpers/BaseTest.sol";
import {AppStorage} from "../contracts/AccountConfigFacets/AppStorage.sol";
import {WritesFacet} from "../contracts/AccountConfigFacets/WritesFacet.sol";

/// @notice Independent regression tests through the production diamond, without forks.
contract UsageKeyManagementExpirationTest is BaseTest {
    uint256 constant MASTER = 101;
    uint256 constant USAGE = 202;
    uint256 constant DEADLINE = 10_000;
    address constant WALLET = address(0x1234);
    address constant NEW_WALLET = address(0x5678);
    uint256 group;

    function setUp() public override {
        super.setUp();
        vm.warp(DEADLINE - 100);
        vm.startPrank(apiPayer);
        writes.newAccount(MASTER, true, "account", "", user);
        uint256[] memory actions = new uint256[](1);
        actions[0] = 301;
        address[] memory wallets = new address[](1);
        wallets[0] = WALLET;
        group = writes.addGroup(MASTER, "original", "", actions, wallets);
        writes.registerWalletDerivation(MASTER, WALLET, 401, "wallet", "");
        _setKey(DEADLINE);
        vm.stopPrank();
    }

    function _setKey(uint256 expiration) internal {
        uint256[] memory groups = new uint256[](1);
        groups[0] = group;
        writes.setUsageApiKey(
            MASTER,
            USAGE,
            expiration,
            0,
            "usage",
            "",
            true,
            true,
            true,
            groups,
            groups,
            groups,
            groups
        );
    }

    // All seven public routes reaching the six management guards. addGroup also
    // exercises both array-valued (batch membership / CID) initializers.
    function _operation(uint256 route, uint256 key) internal view returns (bytes memory) {
        if (route == 0) {
            uint256[] memory actions = new uint256[](10);
            address[] memory wallets = new address[](10);
            for (uint160 i; i < 10; ++i) {
                actions[i] = 500 + i;
                wallets[i] = address(600 + i);
            }
            return abi.encodeCall(WritesFacet.addGroup, (key, "batch", "", actions, wallets));
        }
        if (route == 1) return abi.encodeCall(WritesFacet.removeGroup, (key, group));
        if (route == 2) {
            return
                abi.encodeCall(
                    WritesFacet.registerWalletDerivation, (key, NEW_WALLET, 402, "new", "")
                );
        }
        if (route == 3) return abi.encodeCall(WritesFacet.addPkpToGroup, (key, group, NEW_WALLET));
        if (route == 4) {
            return abi.encodeCall(WritesFacet.removePkpFromGroup, (key, group, WALLET));
        }
        if (route == 5) return abi.encodeCall(WritesFacet.addActionToGroup, (key, group, 302));
        return abi.encodeCall(WritesFacet.removeActionFromGroup, (key, group, 301));
    }

    function _state() internal view returns (bytes32) {
        return keccak256(
            abi.encode(
                views_.listActions(MASTER, 0, 100),
                views_.listApiKeys(MASTER, 0, 100),
                views_.getWalletDerivation(MASTER, WALLET),
                views_.listGroups(MASTER, 0, 100),
                views_.listGroupContents(MASTER, group),
                views_.listGroupContents(MASTER, group + 1),
                views_.listPkps(MASTER, 0, 100),
                views_.pkpCount(),
                views_.allPkpIdsAt(2),
                views_.getWalletDerivation(MASTER, NEW_WALLET),
                views_.getPkpOwnerMaster(NEW_WALLET),
                views_.getPathOwnerMaster(402)
            )
        );
    }

    function _reject(uint256 route, address sender) internal {
        bytes32 beforeState = _state();
        bytes memory callData = _operation(route, USAGE);
        vm.prank(sender);
        (bool ok, bytes memory result) = address(writes).call(callData);
        assertFalse(ok, "expired management call succeeded");
        // Literal signature lets these tests compile and fail against pre-fix code.
        assertEq(result, abi.encodeWithSignature("UsageApiKeyExpired(uint256)", USAGE));
        assertEq(_state(), beforeState, "rejected operation changed state");
    }

    function _allowAll(uint256 key, address sender) internal {
        for (uint256 route; route < 7; ++route) {
            uint256 snapshot = vm.snapshotState();
            bytes32 beforeState = _state();
            bytes memory callData = _operation(route, key);
            vm.prank(sender);
            (bool ok,) = address(writes).call(callData);
            assertTrue(ok, "authorized management call failed");
            assertNotEq(_state(), beforeState, "positive control did not mutate state");
            assertTrue(vm.revertToState(snapshot));
        }
    }

    function test_expiredCreateGroupWithBatchMembersReverts() public {
        vm.warp(DEADLINE + 1);
        _reject(0, apiPayer);
    }

    function test_expiredDeleteGroupReverts() public {
        vm.warp(DEADLINE + 1);
        _reject(1, apiPayer);
    }

    function test_expiredRegisterDerivationRevertsWithoutOwnerBindings() public {
        vm.warp(DEADLINE + 1);
        _reject(2, apiPayer);
    }

    function test_expiredAddMembershipReverts() public {
        vm.warp(DEADLINE + 1);
        _reject(3, apiPayer);
    }

    function test_expiredRemoveMembershipReverts() public {
        vm.warp(DEADLINE + 1);
        _reject(4, apiPayer);
    }

    function test_expiredAddActionCidReverts() public {
        vm.warp(DEADLINE + 1);
        _reject(5, apiPayer);
    }

    function test_expiredRemoveActionCidReverts() public {
        vm.warp(DEADLINE + 1);
        _reject(6, apiPayer);
    }

    function test_allRoutesSucceedOneSecondBeforeExpiration() public {
        vm.warp(DEADLINE - 1);
        assertTrue(views_.canExecuteAction(USAGE, 301));
        _allowAll(USAGE, apiPayer);
    }

    function test_allRoutesRejectAtExactExpiration() public {
        vm.warp(DEADLINE);
        assertFalse(views_.canExecuteAction(USAGE, 301));
        for (uint256 route; route < 7; ++route) {
            _reject(route, apiPayer);
        }
    }

    function test_zeroExpirationNeverExpires() public {
        vm.prank(apiPayer);
        _setKey(0);
        vm.warp(DEADLINE + 20 * 365 days);
        _allowAll(USAGE, apiPayer);
    }

    function test_masterBypassesUsageAndAccountExpiration() public {
        vm.warp(DEADLINE + 20 * 365 days);
        _allowAll(MASTER, apiPayer);
    }

    function test_alreadyExpiredKeyCannotMutate() public {
        vm.prank(apiPayer);
        _setKey(block.timestamp - 1);
        for (uint256 route; route < 7; ++route) {
            _reject(route, apiPayer);
        }
    }

    function test_sovereignAdminCannotUseExpiredUsageScopes() public {
        vm.prank(apiPayer);
        writes.convertToChainSecuredAccount(MASTER, user);
        _allowAll(USAGE, user);
        vm.warp(DEADLINE);
        for (uint256 route; route < 7; ++route) {
            _reject(route, user);
        }
        _allowAll(MASTER, user);
    }

    function test_accountGateRemainsCallerAuthorizationNotKeyAuthorization() public {
        vm.warp(DEADLINE);
        vm.prank(apiPayer);
        assertTrue(views_.accountExistsAndIsMutable(USAGE));
        vm.prank(stranger);
        assertFalse(views_.accountExistsAndIsMutable(USAGE));
        assertEq(views_.listGroups(USAGE, 0, 100).length, 1);
        // Master can renew and remove an expired key; expiry is not account revocation.
        vm.prank(apiPayer);
        _setKey(DEADLINE + 100);
        _allowAll(USAGE, apiPayer);
        vm.warp(DEADLINE + 100);
        vm.prank(apiPayer);
        writes.removeUsageApiKey(MASTER, USAGE);
        assertEq(views_.listApiKeys(MASTER, 0, 100).length, 0);
    }

    function test_unexpiredKeyStillNeedsEveryManagementScope() public {
        uint256[] memory empty = new uint256[](0);
        vm.prank(apiPayer);
        writes.setUsageApiKey(
            MASTER, USAGE, 0, 0, "unscoped", "", false, false, false, empty, empty, empty, empty
        );
        bytes32 beforeState = _state();
        for (uint256 route; route < 7; ++route) {
            bytes memory callData = _operation(route, USAGE);
            vm.prank(apiPayer);
            (bool ok,) = address(writes).call(callData);
            assertFalse(ok, "expiry sentinel bypassed scopes");
            assertEq(_state(), beforeState);
        }
    }

    function test_unauthorizedSenderCannotUseEvenLiveScopes() public {
        for (uint256 route; route < 7; ++route) {
            bytes memory callData = _operation(route, USAGE);
            vm.prank(stranger);
            (bool ok, bytes memory result) = address(writes).call(callData);
            assertFalse(ok);
            assertEq(
                result, abi.encodeWithSelector(AppStorage.NoAccountAccess.selector, USAGE, stranger)
            );
        }
    }

    // Sibling structural routes are master-only, including action registration
    // (distinct from adding an action CID to a group) and batch group updates.
    function _masterOperation(uint256 route, uint256 key) internal view returns (bytes memory) {
        uint256[] memory actions = new uint256[](1);
        actions[0] = 302;
        address[] memory wallets = new address[](1);
        wallets[0] = NEW_WALLET;
        if (route == 0) {
            return
                abi.encodeCall(
                    WritesFacet.updateGroup, (key, group, "updated", "", actions, wallets)
                );
        }
        if (route == 1) {
            return abi.encodeCall(WritesFacet.updateGroupMetadata, (key, group, "updated", ""));
        }
        if (route == 2) return abi.encodeCall(WritesFacet.addAction, (key, "new", "", 302));
        if (route == 3) return abi.encodeCall(WritesFacet.removeAction, (key, 301));
        if (route == 4) {
            return
                abi.encodeCall(WritesFacet.updateActionMetadata, (key, 301, group, "updated", ""));
        }
        if (route == 5) {
            return
                abi.encodeCall(WritesFacet.updateUsageApiKeyMetadata, (key, USAGE, "updated", ""));
        }
        if (route == 6) return abi.encodeCall(WritesFacet.removeUsageApiKey, (key, USAGE));
        return abi.encodeCall(WritesFacet.removeWalletDerivation, (key, WALLET));
    }

    function test_masterOnlySiblingsRemainProtectedAfterExpiration() public {
        vm.prank(apiPayer);
        writes.addAction(MASTER, "original", "", 301);
        vm.warp(DEADLINE);
        for (uint256 route; route < 8; ++route) {
            uint256 snapshot = vm.snapshotState();
            bytes32 beforeState = _state();
            bytes memory callData = _masterOperation(route, USAGE);
            vm.prank(apiPayer);
            (bool ok,) = address(writes).call(callData);
            assertFalse(ok, "usage key reached a master-only mutation");
            assertEq(_state(), beforeState);
            callData = _masterOperation(route, MASTER);
            vm.prank(apiPayer);
            (ok,) = address(writes).call(callData);
            assertTrue(ok, "master-only positive control failed");
            assertNotEq(_state(), beforeState);
            assertTrue(vm.revertToState(snapshot));
        }
    }

    // There is no on-chain batch entrypoint. Model an authorized transaction
    // composing multiple diamond calls and verify a late rejection rolls back
    // earlier successful writes, including global derivation ownership bindings.
    function batch() external {
        writes.registerWalletDerivation(MASTER, NEW_WALLET, 402, "new", "");
        writes.addActionToGroup(USAGE, group, 302);
    }

    function test_composedBatchHasNoPartialWrites() public {
        address[] memory payers = new address[](2);
        payers[0] = apiPayer;
        payers[1] = address(this);
        vm.prank(owner);
        apiConfig.setApiPayers(payers);
        uint256 snapshot = vm.snapshotState();
        this.batch();
        assertEq(views_.getPathOwnerMaster(402), MASTER);
        assertTrue(vm.revertToState(snapshot));
        bytes32 beforeState = _state();
        vm.warp(DEADLINE);
        vm.expectRevert(abi.encodeWithSignature("UsageApiKeyExpired(uint256)", USAGE));
        this.batch();
        assertEq(_state(), beforeState);
    }
}
