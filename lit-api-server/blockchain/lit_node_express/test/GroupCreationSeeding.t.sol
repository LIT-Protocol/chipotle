// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {BaseTest} from "./helpers/BaseTest.sol";
import {AppStorage} from "../contracts/AccountConfigFacets/AppStorage.sol";
import {WritesFacet} from "../contracts/AccountConfigFacets/WritesFacet.sol";

/// @notice A usage key may put wallets and actions into a group it creates only
///         when it holds the grants that addPkpToGroup and addActionToGroup need
///         on that new group (chipotle-private#231). Usage-key writes go through
///         the TEE relay with the key's hash as the account argument, as API-mode
///         writes do in production.
contract GroupCreationSeedingTest is BaseTest {
    uint256 constant MASTER = 1001;
    uint256 constant CREATOR = 2002; // can_create_groups and nothing else
    uint256 constant EXECUTOR = 3003; // execute_in_groups: [0], a separate key
    uint256 constant CID = 4004;
    address constant WALLET = address(0xA11CE); // registered, in no group
    address constant GROUPED = address(0xB0B); // registered, in the existing group
    uint256 existing;

    function setUp() public override {
        super.setUp();
        vm.startPrank(apiPayer);
        writes.newAccount(MASTER, true, "account", "", user);
        writes.registerWalletDerivation(MASTER, WALLET, wp(1), "ungrouped", "");
        writes.registerWalletDerivation(MASTER, GROUPED, wp(2), "grouped", "");
        existing = writes.addGroup(MASTER, "existing", "", ids(CID + 1), wallets(GROUPED));
        setKey(CREATOR, true, none(), none(), none());
        setKey(EXECUTOR, false, none(), none(), ids(0));
        vm.stopPrank();
    }

    function setKey(
        uint256 key,
        bool createGroups,
        uint256[] memory manageActions,
        uint256[] memory addPkps,
        uint256[] memory execute
    ) internal {
        writes.setUsageApiKey(
            MASTER,
            key,
            0,
            0,
            "key",
            "",
            createGroups,
            false,
            false,
            manageActions,
            addPkps,
            none(),
            execute
        );
    }

    function ids(uint256 id) internal pure returns (uint256[] memory list) {
        list = new uint256[](1);
        list[0] = id;
    }

    function wallets(address wallet) internal pure returns (address[] memory list) {
        list = new address[](1);
        list[0] = wallet;
    }

    function none() internal pure returns (uint256[] memory) {
        return new uint256[](0);
    }

    function noWallets() internal pure returns (address[] memory) {
        return new address[](0);
    }

    /// addGroup through the relay, as POST /core/v1/add_group sends it.
    function create(uint256 key, uint256[] memory cids, address[] memory pkps)
        internal
        returns (bool ok, bytes memory result)
    {
        vm.prank(apiPayer);
        (ok, result) = address(writes).call(
            abi.encodeCall(WritesFacet.addGroup, (key, "seeded", "", cids, pkps))
        );
    }

    function deniedPkps(uint256 key, uint256 groupId) internal pure returns (bytes memory) {
        return abi.encodeWithSelector(AppStorage.NotAllowedToAddPkpToGroup.selector, key, groupId);
    }

    function deniedActions(uint256 key, uint256 groupId) internal pure returns (bytes memory) {
        return abi.encodeWithSelector(
            AppStorage.NotAllowedToManageIPFSIdsInGroup.selector, key, groupId
        );
    }

    function assertWalletUse(uint256 key, uint256 cid, address wallet, bool expected)
        internal
        view
    {
        assertEq(views_.canUseWalletInAction(key, cid, wallet), expected, "wallet");
        assertEq(views_.canUseWalletInActionFast(key, cid, wallet), expected, "fast wallet");
        (, bool combined) = views_.canExecuteActionAndUseWallet(key, cid, wallet);
        assertEq(combined, expected, "combined wallet");
    }

    function groupCount() internal view returns (uint256) {
        return views_.listGroups(MASTER, 0, 100).length;
    }

    /// The chain from #276: a create-only key fills a new group, and a separate
    /// wildcard executor can then use a wallet that no group listed before.
    function test_createOnlyKeyCannotHandAWalletToAWildcardExecutor() public {
        uint256 next = existing + 1; // group ids are sequential and never reused
        uint256[2] memory cids = [CID, 0];
        address[2] memory pkps = [WALLET, address(0)];
        for (uint256 i; i < 2; ++i) {
            assertWalletUse(EXECUTOR, CID, WALLET, false);
            (bool ok, bytes memory result) = create(CREATOR, ids(cids[i]), wallets(pkps[i]));
            assertFalse(ok, "create-only key filled a new group");
            assertEq(result, deniedActions(CREATOR, next));
            assertWalletUse(EXECUTOR, CID, WALLET, false);
            assertEq(groupCount(), 1);
        }
    }

    function test_createOnlyKeyCannotListWallets() public {
        uint256 next = existing + 1;
        address[3] memory pkps = [WALLET, GROUPED, address(0)];
        for (uint256 i; i < 3; ++i) {
            (bool ok, bytes memory result) = create(CREATOR, none(), wallets(pkps[i]));
            assertFalse(ok, "create-only key listed a wallet");
            assertEq(result, deniedPkps(CREATOR, next));
        }
        assertEq(groupCount(), 1);
    }

    function test_createOnlyKeyCannotListActions() public {
        uint256 next = existing + 1;
        uint256[2] memory cids = [CID, 0];
        for (uint256 i; i < 2; ++i) {
            (bool ok, bytes memory result) = create(CREATOR, ids(cids[i]), noWallets());
            assertFalse(ok, "create-only key listed an action");
            assertEq(result, deniedActions(CREATOR, next));
        }
        assertEq(groupCount(), 1);
    }

    /// #231's variant: one key holds both group:create and execute(*).
    function test_createAndWildcardExecuteKeyCannotGrantItselfAWallet() public {
        vm.prank(apiPayer);
        setKey(CREATOR, true, none(), none(), ids(0));
        assertWalletUse(CREATOR, CID, WALLET, false);
        (bool ok,) = create(CREATOR, ids(0), wallets(address(0)));
        assertFalse(ok, "key granted itself every wallet");
        assertWalletUse(CREATOR, CID, WALLET, false);
        assertWalletUse(CREATOR, CID, GROUPED, false);
    }

    /// Grants are per group. Grants on the existing group must not cover the
    /// one being created, which pins the group id the check uses.
    function test_grantsOnAnExistingGroupDoNotCoverTheNewGroup() public {
        uint256 next = existing + 1;
        vm.prank(apiPayer);
        setKey(CREATOR, true, ids(existing), ids(existing), none());

        (bool ok, bytes memory result) = create(CREATOR, none(), wallets(WALLET));
        assertFalse(ok, "existing-group grant covered the new group's wallets");
        assertEq(result, deniedPkps(CREATOR, next));
        (ok, result) = create(CREATOR, ids(CID), noWallets());
        assertFalse(ok, "existing-group grant covered the new group's actions");
        assertEq(result, deniedActions(CREATOR, next));

        // Control: the same grants work on the group they name.
        vm.startPrank(apiPayer);
        writes.addActionToGroup(CREATOR, existing, CID);
        writes.addPkpToGroup(CREATOR, existing, WALLET);
        vm.stopPrank();
        assertWalletUse(EXECUTOR, CID, WALLET, true);
    }

    /// Each array needs its own grant: add_pkp_to_groups for wallets,
    /// manage_ipfs_ids_in_groups for actions.
    function test_grantOnTheNewGroupCoversOnlyItsOwnArray() public {
        uint256 next = existing + 1;
        uint256 snapshot = vm.snapshotState();

        vm.prank(apiPayer);
        setKey(CREATOR, true, none(), ids(next), none());
        (bool ok, bytes memory result) = create(CREATOR, ids(CID), noWallets());
        assertFalse(ok, "add_pkp_to_groups let the key list actions");
        assertEq(result, deniedActions(CREATOR, next));
        (ok, result) = create(CREATOR, none(), wallets(WALLET));
        assertTrue(ok, "add_pkp_to_groups on the new group did not let the key list wallets");
        assertEq(abi.decode(result, (uint256)), next);
        assertTrue(vm.revertToState(snapshot));

        vm.prank(apiPayer);
        setKey(CREATOR, true, ids(next), none(), none());
        (ok, result) = create(CREATOR, none(), wallets(WALLET));
        assertFalse(ok, "manage_ipfs_ids_in_groups let the key list wallets");
        assertEq(result, deniedPkps(CREATOR, next));
        (ok, result) = create(CREATOR, ids(CID), noWallets());
        assertTrue(ok, "manage_ipfs_ids_in_groups on the new group did not let the key list actions");
        assertEq(abi.decode(result, (uint256)), next);
    }

    /// With both grants, filling the group at creation ends in the same state as
    /// creating it empty and then calling addActionToGroup and addPkpToGroup.
    function test_initialContentsMatchTheGranularWrites() public {
        uint256 next = existing + 1;
        vm.prank(apiPayer);
        setKey(CREATOR, true, ids(next), ids(next), none());
        uint256 snapshot = vm.snapshotState();

        (bool ok,) = create(CREATOR, ids(CID), wallets(WALLET));
        assertTrue(ok, "key holding both grants could not fill the new group");
        bytes32 filled = keccak256(abi.encode(views_.listGroupContents(MASTER, next)));
        assertWalletUse(EXECUTOR, CID, WALLET, true);
        assertTrue(vm.revertToState(snapshot));

        (ok,) = create(CREATOR, none(), noWallets());
        assertTrue(ok);
        vm.startPrank(apiPayer);
        writes.addActionToGroup(CREATOR, next, CID);
        writes.addPkpToGroup(CREATOR, next, WALLET);
        vm.stopPrank();
        assertEq(keccak256(abi.encode(views_.listGroupContents(MASTER, next))), filled);
        assertWalletUse(EXECUTOR, CID, WALLET, true);
    }

    /// `[0]` (the dashboard's "All Groups") is an exact id in the per-group
    /// management checks: addPkpToGroup and addActionToGroup refuse it on an
    /// existing group, and filling a new group at creation follows them.
    function test_zeroScopeDoesNotCoverTheNewGroup() public {
        uint256 next = existing + 1;
        vm.prank(apiPayer);
        setKey(CREATOR, true, ids(0), ids(0), ids(0));

        vm.prank(apiPayer);
        (bool ok, bytes memory result) = address(writes).call(
            abi.encodeCall(WritesFacet.addPkpToGroup, (CREATOR, existing, WALLET))
        );
        assertFalse(ok, "[0] covered addPkpToGroup");
        assertEq(result, deniedPkps(CREATOR, existing));
        vm.prank(apiPayer);
        (ok, result) = address(writes).call(
            abi.encodeCall(WritesFacet.addActionToGroup, (CREATOR, existing, CID))
        );
        assertFalse(ok, "[0] covered addActionToGroup");
        assertEq(result, deniedActions(CREATOR, existing));

        (ok, result) = create(CREATOR, none(), wallets(WALLET));
        assertFalse(ok, "[0] let the key list wallets");
        assertEq(result, deniedPkps(CREATOR, next));
        (ok, result) = create(CREATOR, ids(CID), noWallets());
        assertFalse(ok, "[0] let the key list actions");
        assertEq(result, deniedActions(CREATOR, next));
        assertWalletUse(CREATOR, CID, WALLET, false);
    }

    function test_createOnlyKeyCanStillCreateAnEmptyGroup() public {
        (bool ok, bytes memory result) = create(CREATOR, none(), noWallets());
        assertTrue(ok, "create-only key could not create an empty group");
        uint256 groupId = abi.decode(result, (uint256));
        assertEq(groupId, existing + 1);
        assertEq(groupCount(), 2);
        assertWalletUse(EXECUTOR, CID, WALLET, false);

        // The account key fills it.
        vm.startPrank(apiPayer);
        writes.addActionToGroup(MASTER, groupId, CID);
        writes.addPkpToGroup(MASTER, groupId, WALLET);
        vm.stopPrank();
        assertWalletUse(EXECUTOR, CID, WALLET, true);
    }

    function test_accountOwnerCanStillListAnything() public {
        vm.prank(apiPayer);
        uint256 groupId = writes.addGroup(MASTER, "owner", "", ids(0), wallets(address(0)));
        assertWalletUse(EXECUTOR, CID, WALLET, true);
        vm.prank(apiPayer);
        writes.removeGroup(MASTER, groupId);
        assertWalletUse(EXECUTOR, CID, WALLET, false);

        // ChainSecured: the admin wallet writes directly, under its own alias.
        vm.prank(apiPayer);
        writes.convertToChainSecuredAccount(MASTER, user);
        vm.prank(user);
        writes.addGroup(apiKeyHashOf(user), "owner", "", ids(CID), wallets(WALLET));
        assertWalletUse(EXECUTOR, CID, WALLET, true);
    }
}
