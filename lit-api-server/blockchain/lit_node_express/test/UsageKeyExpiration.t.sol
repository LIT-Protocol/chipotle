// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;
import {BaseTest} from "./helpers/BaseTest.sol";
import {AppStorage} from "../contracts/AccountConfigFacets/AppStorage.sol";

/// @notice Regression tests for #68 — an expired usage API key retained
///         management-plane authority because the six SecurityLib management
///         guards never read `expiration`. The execute/read views were already
///         gated by #31/#703; these tests lock in the residual management-plane
///         fix (differential: pre-expiry succeeds, post-expiry management writes
///         revert while execution is denied).
contract UsageKeyExpirationTest is BaseTest {
    uint256 constant MASTER = 12345;
    uint256 constant USAGE = 67890;
    uint256 constant GROUP = 1; // first group created below
    uint256 constant ACTION = uint256(keccak256("action-cid"));
    uint256 constant EXPIRY_WINDOW = 7 days;

    uint256 internal expiration;

    function setUp() public override {
        super.setUp();
        expiration = block.timestamp + EXPIRY_WINDOW;

        vm.startPrank(apiPayer);
        writes.newAccount(MASTER, true, "managed", "", user);

        // Master creates a group the usage key is scoped to.
        uint256[] memory emptyU = new uint256[](0);
        address[] memory emptyA = new address[](0);
        uint256 gid = writes.addGroup(MASTER, "g", "", emptyU, emptyA);
        assertEq(gid, GROUP);

        // Usage key with the full management scope set, expiring in 7 days.
        uint256[] memory groups = new uint256[](1);
        groups[0] = GROUP;
        writes.setUsageApiKey(
            MASTER,
            USAGE,
            expiration,
            0,
            "key",
            "",
            true, // createGroups
            true, // deleteGroups
            true, // createPKPs
            groups, // manageIPFSIdsInGroups
            groups, // addPkpToGroups
            groups, // removePkpFromGroups
            groups // executeInGroups
        );
        vm.stopPrank();
    }

    // ---- pre-expiry: the usage key works as scoped ----

    function test_beforeExpiry_managementWritesSucceed() public {
        vm.warp(expiration - 1);
        uint256[] memory emptyU = new uint256[](0);
        address[] memory emptyA = new address[](0);

        vm.startPrank(apiPayer);
        writes.addGroup(USAGE, "g2", "", emptyU, emptyA);
        writes.addActionToGroup(USAGE, GROUP, ACTION);
        vm.stopPrank();

        assertTrue(views_.canExecuteAction(USAGE, ACTION));
    }

    // ---- at/after expiry: management writes revert, execution is denied ----

    function test_expiredUsageKeyCannotCreateGroup() public {
        vm.warp(expiration); // block.timestamp >= expiration
        uint256[] memory emptyU = new uint256[](0);
        address[] memory emptyA = new address[](0);

        vm.prank(apiPayer);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.UsageApiKeyExpired.selector, USAGE));
        writes.addGroup(USAGE, "g2", "", emptyU, emptyA);
    }

    function test_expiredUsageKeyCannotDeleteGroup() public {
        vm.warp(expiration);
        vm.prank(apiPayer);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.UsageApiKeyExpired.selector, USAGE));
        writes.removeGroup(USAGE, GROUP);
    }

    function test_expiredUsageKeyCannotRegisterWalletDerivation() public {
        vm.warp(expiration);
        vm.prank(apiPayer);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.UsageApiKeyExpired.selector, USAGE));
        writes.registerWalletDerivation(USAGE, makeAddr("pkp"), uint256(keccak256("path")), "w", "");
    }

    function test_expiredUsageKeyCannotAddPkpToGroup() public {
        vm.warp(expiration);
        vm.prank(apiPayer);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.UsageApiKeyExpired.selector, USAGE));
        writes.addPkpToGroup(USAGE, GROUP, makeAddr("pkp"));
    }

    function test_expiredUsageKeyCannotRemovePkpFromGroup() public {
        vm.warp(expiration);
        vm.prank(apiPayer);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.UsageApiKeyExpired.selector, USAGE));
        writes.removePkpFromGroup(USAGE, GROUP, makeAddr("pkp"));
    }

    /// @notice The High-ceiling angle: action-CID poisoning via manageIPFSIds.
    function test_expiredUsageKeyCannotPoisonGroupActions() public {
        vm.warp(expiration);
        vm.startPrank(apiPayer);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.UsageApiKeyExpired.selector, USAGE));
        writes.addActionToGroup(USAGE, GROUP, ACTION);

        vm.expectRevert(abi.encodeWithSelector(AppStorage.UsageApiKeyExpired.selector, USAGE));
        writes.removeActionFromGroup(USAGE, GROUP, ACTION);
        vm.stopPrank();
    }

    /// @notice Execution was already denied by #31/#703; confirm the two planes
    ///         now agree once the key expires.
    function test_expiredUsageKeyCannotExecute() public {
        vm.startPrank(apiPayer);
        writes.addActionToGroup(MASTER, GROUP, ACTION);
        vm.stopPrank();

        assertTrue(views_.canExecuteAction(USAGE, ACTION)); // pre-expiry
        vm.warp(expiration);
        assertFalse(views_.canExecuteAction(USAGE, ACTION)); // post-expiry
    }

    // ---- master key and never-expiring keys are unaffected ----

    function test_masterKeyUnaffectedByExpiryWindow() public {
        vm.warp(expiration + 365 days);
        uint256[] memory emptyU = new uint256[](0);
        address[] memory emptyA = new address[](0);
        vm.prank(apiPayer);
        writes.addGroup(MASTER, "still-works", "", emptyU, emptyA);
    }

    function test_neverExpiringUsageKeyStillManages() public {
        // expiration == 0 is the "never expires" sentinel.
        uint256 forever = 55555;
        uint256[] memory groups = new uint256[](1);
        groups[0] = GROUP;
        vm.startPrank(apiPayer);
        writes.setUsageApiKey(
            MASTER, forever, 0, 0, "forever", "", true, true, true, groups, groups, groups, groups
        );
        vm.warp(expiration + 365 days);
        uint256[] memory emptyU = new uint256[](0);
        address[] memory emptyA = new address[](0);
        writes.addGroup(forever, "ok", "", emptyU, emptyA);
        vm.stopPrank();
    }
}
