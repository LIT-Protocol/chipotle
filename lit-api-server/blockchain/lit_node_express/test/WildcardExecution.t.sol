// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {BaseTest} from "./helpers/BaseTest.sol";
import {AppStorage} from "../contracts/AccountConfigFacets/AppStorage.sol";

contract WildcardExecutionTest is BaseTest {
    uint256 constant USAGE = 123456;
    uint256 constant CID = 789;
    address constant WALLET = address(0xBEEF);
    uint256 master;

    function setUp() public override {
        super.setUp();
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "");
        master = apiKeyHashOf(user);
        setKey(0, 0);
    }

    function setKey(uint256 groupId, uint256 expiration) internal {
        uint256[] memory empty = new uint256[](0);
        uint256[] memory execute = new uint256[](1);
        execute[0] = groupId;
        vm.prank(user);
        writes.setUsageApiKey(
            master,
            USAGE,
            expiration,
            0,
            "execution",
            "",
            false,
            false,
            false,
            empty,
            empty,
            empty,
            execute
        );
    }

    function addGroup(address admin, uint256 cid, address wallet) internal returns (uint256) {
        uint256[] memory cids = new uint256[](1);
        cids[0] = cid;
        address[] memory wallets = new address[](1);
        wallets[0] = wallet;
        vm.prank(admin);
        return writes.addGroup(apiKeyHashOf(admin), "permissions", "", cids, wallets);
    }

    function assertPermissions(
        uint256 key,
        uint256 cid,
        address wallet,
        bool execute,
        bool useWallet
    ) internal view {
        assertEq(views_.canExecuteAction(key, cid), execute, "execution");
        assertEq(views_.canExecuteActionFast(key, cid), execute, "fast execution");
        assertEq(views_.canUseWalletInAction(key, cid, wallet), useWallet, "wallet");
        assertEq(views_.canUseWalletInActionFast(key, cid, wallet), useWallet, "fast wallet");
        (bool combinedExecute, bool combinedWallet) =
            views_.canExecuteActionAndUseWallet(key, cid, wallet);
        assertEq(combinedExecute, execute, "combined execution");
        assertEq(combinedWallet, useWallet, "combined wallet");
    }

    function test_wildcardExecutesWithoutAnyGroupsButCannotUseWallet() public view {
        assertPermissions(USAGE, CID, WALLET, true, false);
        uint256[] memory empty = new uint256[](0);
        assertFalse(views_.apiKeyCanExecuteForAnyGroup(USAGE, empty));
    }

    function test_wildcardCannotUseForeignWalletEvenWithMatchingAction() public {
        vm.prank(stranger);
        writes.newChainSecuredAccount("bob", "");
        addGroup(stranger, CID, WALLET);
        // A foreign group must never authorize wallet use by Alice's key.
        assertPermissions(USAGE, CID, WALLET, true, false);
        address ownWallet = address(0xCAFE);
        addGroup(user, CID, ownWallet);
        assertPermissions(USAGE, CID, ownWallet, true, true);
        assertPermissions(USAGE, CID, WALLET, true, false);
    }

    function testFuzz_ownGroupListingForeignWalletDoesNotGrantDerivation(
        bool allActions,
        bool allWallets
    ) public {
        vm.prank(stranger);
        writes.newChainSecuredAccount("victim", "");
        uint256 victimMaster = apiKeyHashOf(stranger);
        vm.prank(stranger);
        writes.registerWalletDerivation(victimMaster, WALLET, 42, "victim wallet", "");

        // Group membership is not ownership. Even an explicit foreign address
        // (or an all-wallets wildcard) can pass all five permission views.
        addGroup(user, allActions ? 0 : CID, allWallets ? address(0) : WALLET);
        assertPermissions(USAGE, CID, WALLET, true, true);
        assertEq(views_.getPkpOwnerMaster(WALLET), victimMaster);
        assertEq(views_.getWalletDerivation(victimMaster, WALLET), 42);
        assertEq(views_.getWalletDerivation(USAGE, WALLET), 0);

        // The caller cannot attach the victim's public derivation path to its
        // own account. At runtime, the zero/unregistered path also cannot pass
        // get_verified_client_key's address check (covered in the API suite).
        vm.prank(user);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector, "PKP owned by another account"
            )
        );
        writes.registerWalletDerivation(master, WALLET, 42, "stolen", "");
        assertEq(views_.getWalletDerivation(USAGE, WALLET), 0);
    }

    function test_ownGroupCannotReleaseStaleForeignWalletDerivation() public {
        vm.prank(user);
        writes.registerWalletDerivation(master, WALLET, 42, "legacy registration", "");
        addGroup(user, CID, WALLET);
        assertPermissions(USAGE, CID, WALLET, true, true);
        assertEq(views_.getWalletDerivation(USAGE, WALLET), 42);

        // Simulate an old cross-account registration after the real owner's
        // binding is backfilled, as in AccountsTest. Check slot layout first.
        bytes32 base = keccak256("com.litprotocol.accountconfig.storage");
        bytes32 ownerSlot = keccak256(abi.encode(WALLET, bytes32(uint256(base) + 18)));
        assertEq(uint256(vm.load(address(views_), ownerSlot)), master);
        uint256 victimMaster = apiKeyHashOf(stranger);
        vm.store(address(views_), ownerSlot, bytes32(victimMaster));
        assertEq(views_.getPkpOwnerMaster(WALLET), victimMaster);

        // Authorization still succeeds; the separate key-resolution boundary
        // must reject the stale record when called with the usage key hash.
        assertPermissions(USAGE, CID, WALLET, true, true);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector, "PKP owned by another account"
            )
        );
        views_.getWalletDerivation(USAGE, WALLET);
    }

    function test_wildcardRequiresActionAndWalletInSameGroup() public {
        addGroup(user, CID, address(0xCAFE));
        addGroup(user, CID + 1, WALLET);
        assertPermissions(USAGE, CID, WALLET, true, false);
        uint256 groupId = addGroup(user, CID, WALLET);
        assertPermissions(USAGE, CID, WALLET, true, true);
        vm.prank(user);
        writes.removeGroup(master, groupId);
        assertPermissions(USAGE, CID, WALLET, true, false);
    }

    function test_wildcardActionEntryAllowsUnregisteredCodeWithPermittedWallet() public {
        addGroup(user, 0, WALLET);
        assertPermissions(USAGE, CID, WALLET, true, true);
        assertPermissions(USAGE, CID + 1, WALLET, true, true);
        assertPermissions(USAGE, CID, address(0xCAFE), true, false);
    }

    function test_walletWildcardStillRequiresMatchingAction() public {
        addGroup(user, CID, address(0));
        assertPermissions(USAGE, CID, WALLET, true, true);
        assertPermissions(USAGE, CID + 1, WALLET, true, false);
    }

    function test_scopedKeyStillRequiresGrantedGroup() public {
        uint256 allowed = addGroup(user, CID, WALLET);
        addGroup(user, CID + 1, WALLET);
        setKey(allowed, 0);
        assertPermissions(USAGE, CID, WALLET, true, true);
        assertPermissions(USAGE, CID + 1, WALLET, false, false);
        assertPermissions(USAGE, CID, address(0xCAFE), true, false);
        assertPermissions(USAGE, CID + 2, WALLET, false, false);
    }

    function test_expiredWildcardCannotExecuteEvenWithoutGroups() public {
        setKey(0, block.timestamp + 1);
        vm.warp(block.timestamp + 1);
        assertPermissions(USAGE, CID, WALLET, false, false);
        addGroup(user, CID, WALLET);
        assertPermissions(USAGE, CID, WALLET, false, false);
    }

    function test_masterAndUnknownKeysDoNotGainExecutionPermission() public {
        addGroup(user, CID, WALLET);
        assertPermissions(master, CID, WALLET, false, false);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.AccountDoesNotExist.selector, 999));
        views_.canExecuteAction(999, CID);
    }

    function test_revokedWildcardCannotExecute() public {
        vm.prank(user);
        writes.removeUsageApiKey(master, USAGE);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.AccountDoesNotExist.selector, USAGE));
        views_.canExecuteAction(USAGE, CID);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.AccountDoesNotExist.selector, USAGE));
        views_.canExecuteActionFast(USAGE, CID);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.AccountDoesNotExist.selector, USAGE));
        views_.canExecuteActionAndUseWallet(USAGE, CID, WALLET);
    }

    function testFuzz_wildcardUnregisteredActionCannotUseWallet(uint256 cid, address wallet)
        public
        view
    {
        assertPermissions(USAGE, cid, wallet, true, false);
    }
}
