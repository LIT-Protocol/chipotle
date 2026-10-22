// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {Test} from "forge-std/Test.sol";
import {BaseTest} from "./helpers/BaseTest.sol";
import {AccountConfig} from "../contracts/AccountConfig.sol";
import {AppStorage} from "../contracts/AccountConfigFacets/AppStorage.sol";
import {ViewsFacet} from "../contracts/AccountConfigFacets/ViewsFacet.sol";
import {WritesFacet} from "../contracts/AccountConfigFacets/WritesFacet.sol";
import {BillingFacet} from "../contracts/AccountConfigFacets/BillingFacet.sol";
import {APIConfigFacet} from "../contracts/AccountConfigFacets/APIConfigFacet.sol";
import {DiamondInit} from "../contracts/AccountConfigFacets/DiamondInit.sol";
import {DiamondCutFacet} from "../libraries/diamond/DiamondCutFacet.sol";
import {DiamondLoupeFacet} from "../libraries/diamond/DiamondLoupeFacet.sol";
import {OwnershipFacet} from "../libraries/diamond/OwnershipFacet.sol";
import {IDiamond} from "../interfaces/IDiamond.sol";
import {DiamondDeploy} from "./helpers/DiamondDeploy.sol";
import {LegacyViewsFacet} from "./legacy/LegacyViewsFacet.sol";
import {LegacyWritesFacet} from "./legacy/LegacyWritesFacet.sol";
import {LegacyAPIConfigFacet} from "./legacy/LegacyAPIConfigFacet.sol";

/// @notice #147. convertToChainSecuredAccount and
///         transferChainSecuredAccountOwnership keep the master apiKeyHash and
///         register keccak256(abi.encodePacked(newAdmin)) as an alias of it.
///         The dashboard and core_sdk.js send that alias as the account hash
///         for every admin write, so the current admin's own alias must be
///         accepted wherever the master hash is. Usage keys, previous admins
///         and stale aliases keep their existing restrictions.
contract AdminWalletAliasTest is BaseTest {
    uint256 internal constant MANAGED = uint256(keccak256("issue-147-managed"));
    uint256 internal constant UNMANAGED = uint256(keccak256("issue-147-unmanaged"));
    uint256 internal constant USAGE = uint256(keccak256("issue-147-predecessor-usage-key"));
    uint256 internal constant NEW_USAGE = uint256(keccak256("issue-147-successor-usage-key"));
    uint256 internal constant ACTION = 301;
    uint256 internal constant NEW_ACTION = 302;
    address internal constant WALLET = address(0xA11CE);
    address internal constant NEW_WALLET = address(0xB0B);
    uint256 internal constant ROUTES = 16;

    address internal successor = makeAddr("successor");
    address internal thirdAdmin = makeAddr("thirdAdmin");

    uint256 internal master;
    uint256 internal group;

    // ---------------------------------------------------------------------
    // Account fixtures
    // ---------------------------------------------------------------------

    /// One action, one group holding it and WALLET, WALLET registered, and a
    /// usage key (issued by the current admin) that can execute in the group.
    function _seed(address actor, uint256 accountHash) internal {
        uint256[] memory actions = new uint256[](1);
        actions[0] = ACTION;
        address[] memory wallets = new address[](1);
        wallets[0] = WALLET;
        uint256[] memory empty = new uint256[](0);
        vm.startPrank(actor);
        writes.addAction(accountHash, "action", "", ACTION);
        group = writes.addGroup(accountHash, "group", "", actions, wallets);
        writes.registerWalletDerivation(accountHash, WALLET, wp(401), "wallet", "");
        uint256[] memory groups = new uint256[](1);
        groups[0] = group;
        writes.setUsageApiKey(
            accountHash,
            USAGE,
            0,
            0,
            "predecessor",
            "",
            false,
            false,
            false,
            empty,
            empty,
            empty,
            groups
        );
        vm.stopPrank();
    }

    /// newChainSecuredAccount by `user`, then transferred to `successor`.
    function _transferred() internal {
        vm.prank(user);
        writes.newChainSecuredAccount("account", "");
        master = apiKeyHashOf(user);
        _seed(user, master);
        vm.prank(user);
        writes.transferChainSecuredAccountOwnership(master, successor);
    }

    /// Managed (API-mode) account administered by `user`, converted to
    /// ChainSecured with `successor` as the new admin.
    function _converted() internal {
        vm.prank(apiPayer);
        writes.newAccount(MANAGED, true, "managed", "", user);
        master = MANAGED;
        _seed(apiPayer, MANAGED);
        vm.prank(apiPayer);
        writes.convertToChainSecuredAccount(MANAGED, successor);
    }

    // ---------------------------------------------------------------------
    // Every admin write the dashboard / core_sdk.js can send
    // ---------------------------------------------------------------------

    function _op(uint256 route, uint256 key) internal view returns (bytes memory) {
        uint256[] memory actions = new uint256[](1);
        actions[0] = NEW_ACTION;
        address[] memory wallets = new address[](1);
        wallets[0] = NEW_WALLET;
        uint256[] memory groups = new uint256[](1);
        groups[0] = group;
        // Usage-scoped writes: a non-master hash goes through SecurityLib.can*.
        if (route == 0) {
            return abi.encodeCall(WritesFacet.addGroup, (key, "new", "", actions, wallets));
        }
        if (route == 1) return abi.encodeCall(WritesFacet.removeGroup, (key, group));
        if (route == 2) {
            return abi.encodeCall(
                WritesFacet.registerWalletDerivation, (key, NEW_WALLET, wp(402), "new", "")
            );
        }
        if (route == 3) return abi.encodeCall(WritesFacet.addPkpToGroup, (key, group, NEW_WALLET));
        if (route == 4) {
            return abi.encodeCall(WritesFacet.removePkpFromGroup, (key, group, WALLET));
        }
        if (route == 5) {
            return abi.encodeCall(WritesFacet.addActionToGroup, (key, group, NEW_ACTION));
        }
        if (route == 6) {
            return abi.encodeCall(WritesFacet.removeActionFromGroup, (key, group, ACTION));
        }
        // Master-only writes: revertIfNotMasterAccount.
        if (route == 7) {
            return abi.encodeCall(
                WritesFacet.setUsageApiKey,
                (key, NEW_USAGE, 0, 0, "new", "", true, true, true, groups, groups, groups, groups)
            );
        }
        if (route == 8) {
            return
                abi.encodeCall(
                    WritesFacet.updateGroup, (key, group, "updated", "", actions, wallets)
                );
        }
        if (route == 9) {
            return abi.encodeCall(WritesFacet.updateGroupMetadata, (key, group, "updated", ""));
        }
        if (route == 10) {
            return abi.encodeCall(WritesFacet.addAction, (key, "new", "", NEW_ACTION));
        }
        if (route == 11) return abi.encodeCall(WritesFacet.removeAction, (key, ACTION));
        if (route == 12) {
            return
                abi.encodeCall(
                    WritesFacet.updateActionMetadata, (key, ACTION, group, "updated", "")
                );
        }
        if (route == 13) {
            return
                abi.encodeCall(WritesFacet.updateUsageApiKeyMetadata, (key, USAGE, "updated", ""));
        }
        if (route == 14) return abi.encodeCall(WritesFacet.removeUsageApiKey, (key, USAGE));
        return abi.encodeCall(WritesFacet.removeWalletDerivation, (key, WALLET));
    }

    function _state() internal view returns (bytes32) {
        return keccak256(
            abi.encode(
                views_.getAccountWalletAddress(master),
                views_.listGroups(master, 0, 100),
                views_.listGroupContents(master, group),
                views_.listGroupContents(master, group + 1),
                views_.listActions(master, 0, 100),
                views_.listApiKeys(master, 0, 100),
                views_.listPkps(master, 0, 100),
                views_.getWalletDerivation(master, WALLET),
                views_.getWalletDerivation(master, NEW_WALLET),
                views_.getPkpOwnerMaster(NEW_WALLET),
                views_.getPathOwnerMaster(wp(402)),
                views_.pkpCount()
            )
        );
    }

    /// Every admin write sent by `sender` with `key` succeeds and leaves
    /// exactly the state the same write sent with the master hash leaves.
    function _assertActsAsMaster(uint256 key, address sender) internal {
        for (uint256 route; route < ROUTES; ++route) {
            uint256 snapshot = vm.snapshotState();
            bytes32 beforeState = _state();
            vm.prank(sender);
            (bool masterOk,) = address(writes).call(_op(route, master));
            assertTrue(
                masterOk,
                string.concat("master positive control failed, route ", vm.toString(route))
            );
            bytes32 viaMaster = _state();
            assertNotEq(viaMaster, beforeState, "master positive control did not mutate state");
            assertTrue(vm.revertToState(snapshot));

            snapshot = vm.snapshotState();
            vm.prank(sender);
            (bool ok, bytes memory result) = address(writes).call(_op(route, key));
            if (!ok) emit log_named_bytes("revert data", result);
            assertTrue(ok, string.concat("admin wallet alias rejected, route ", vm.toString(route)));
            assertEq(_state(), viaMaster, "alias write differs from the master write");
            assertTrue(vm.revertToState(snapshot));
        }
    }

    /// Every admin write sent by `sender` with `key` reverts (with `expected`
    /// when it is non-empty) and changes nothing.
    function _assertRejected(uint256 key, address sender, bytes memory expected) internal {
        bytes32 beforeState = _state();
        for (uint256 route; route < ROUTES; ++route) {
            vm.prank(sender);
            (bool ok, bytes memory result) = address(writes).call(_op(route, key));
            assertFalse(ok, string.concat("write accepted, route ", vm.toString(route)));
            if (expected.length != 0) assertEq(result, expected);
            assertEq(_state(), beforeState, "rejected write changed state");
        }
    }

    // ---------------------------------------------------------------------
    // The current admin's own alias acts as the master hash
    // ---------------------------------------------------------------------

    function test_transferredAdminAliasActsAsMaster() public {
        _transferred();
        _assertActsAsMaster(apiKeyHashOf(successor), successor);
    }

    function test_convertedAdminAliasActsAsMaster() public {
        _converted();
        _assertActsAsMaster(apiKeyHashOf(successor), successor);
    }

    /// Conversion followed by a transfer: the latest admin's alias works.
    function test_convertThenTransferLatestAdminAliasActsAsMaster() public {
        _converted();
        vm.prank(successor);
        writes.transferChainSecuredAccountOwnership(apiKeyHashOf(successor), thirdAdmin);
        _assertActsAsMaster(apiKeyHashOf(thirdAdmin), thirdAdmin);
    }

    /// The successor revokes a usage key the previous admin issued, using the
    /// hash the dashboard sends, and the revoked key stops authorizing.
    function test_successorRevokesPredecessorUsageKeyWithOwnAlias() public {
        _transferred();
        uint256 successorAlias = apiKeyHashOf(successor);
        assertTrue(views_.canExecuteAction(USAGE, ACTION));
        // Views already resolve the alias to the master account.
        assertEq(views_.listApiKeys(successorAlias, 0, 100).length, 1);

        vm.expectEmit(address(writes));
        emit WritesFacet.UsageApiKeyRemoved(master, USAGE);
        vm.prank(successor);
        writes.removeUsageApiKey(successorAlias, USAGE);

        assertEq(views_.listApiKeys(successorAlias, 0, 100).length, 0);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.AccountDoesNotExist.selector, USAGE));
        views_.canExecuteAction(USAGE, ACTION);
    }

    /// Alias writes emit the master hash, as master writes do.
    function test_aliasWritesEmitTheMasterHash() public {
        _converted();
        uint256 successorAlias = apiKeyHashOf(successor);

        vm.expectEmit(address(writes));
        emit WritesFacet.ActionAdded(master, NEW_ACTION);
        vm.prank(successor);
        writes.addAction(successorAlias, "new", "", NEW_ACTION);

        vm.expectEmit(address(writes));
        emit WritesFacet.GroupAdded(master, group + 1);
        vm.prank(successor);
        writes.addGroup(successorAlias, "new", "", new uint256[](0), new address[](0));

        vm.expectEmit(address(writes));
        emit WritesFacet.WalletDerivationRegistered(master, NEW_WALLET, wp(402));
        vm.prank(successor);
        writes.registerWalletDerivation(successorAlias, NEW_WALLET, wp(402), "new", "");
        assertEq(views_.getPkpOwnerMaster(NEW_WALLET), master);
        assertEq(views_.getPathOwnerMaster(wp(402)), master);
    }

    // ---------------------------------------------------------------------
    // Nothing else gains authority
    // ---------------------------------------------------------------------

    function test_previousAdminLosesEveryWriteAfterTransfer() public {
        _transferred();
        // For a newChainSecuredAccount, keccak256(previous admin) IS the master.
        _assertRejected(
            master, user, abi.encodeWithSelector(AppStorage.NoAccountAccess.selector, master, user)
        );
    }

    function test_previousAdminsLoseEveryWriteAfterConvertThenTransfer() public {
        _converted();
        uint256 successorAlias = apiKeyHashOf(successor);
        vm.prank(successor);
        writes.transferChainSecuredAccountOwnership(successorAlias, thirdAdmin);

        _assertRejected(
            successorAlias,
            successor,
            abi.encodeWithSelector(AppStorage.NoAccountAccess.selector, successorAlias, successor)
        );
        _assertRejected(
            master,
            successor,
            abi.encodeWithSelector(AppStorage.NoAccountAccess.selector, master, successor)
        );
        _assertRejected(
            master, user, abi.encodeWithSelector(AppStorage.NoAccountAccess.selector, master, user)
        );
        _assertRejected(
            master,
            apiPayer,
            abi.encodeWithSelector(AppStorage.NoAccountAccess.selector, master, apiPayer)
        );
    }

    /// A previous admin's alias is not the current admin's identity: it keeps
    /// the non-master treatment (no scopes), even when the current admin sends it.
    function test_previousAdminAliasStaysRestrictedForCurrentAdmin() public {
        _converted();
        uint256 successorAlias = apiKeyHashOf(successor);
        vm.prank(successor);
        writes.transferChainSecuredAccountOwnership(successorAlias, thirdAdmin);
        _assertRejected(successorAlias, thirdAdmin, "");
    }

    /// Usage keys keep their scopes when the admin wallet itself sends them.
    function test_usageKeyKeepsItsScopesForTheAdminSender() public {
        _transferred();
        _assertRejected(USAGE, successor, "");
    }

    /// A managed account has no admin-wallet alias. A usage key whose hash
    /// equals keccak256(admin wallet) keeps its (empty) scopes for both the
    /// admin wallet and the api_payer.
    function test_managedUsageKeyAtAdminWalletHashKeepsItsScopes() public {
        vm.prank(apiPayer);
        writes.newAccount(MANAGED, true, "managed", "", user);
        master = MANAGED;
        _seed(apiPayer, MANAGED);
        uint256 walletHash = apiKeyHashOf(user);
        uint256[] memory empty = new uint256[](0);
        vm.prank(apiPayer);
        writes.setUsageApiKey(
            MANAGED,
            walletHash,
            0,
            0,
            "unscoped",
            "",
            false,
            false,
            false,
            empty,
            empty,
            empty,
            empty
        );
        _assertRejected(walletHash, user, "");
        _assertRejected(walletHash, apiPayer, "");
    }

    /// An api_payer can create an unmanaged account whose master hash is not
    /// keccak256(admin wallet), and the admin can then register that hash as a
    /// usage key. The hash passes every other check in adminWriteAccountHash
    /// (unmanaged account, sent by the admin, equals keccak256(sender)), so
    /// only the usage-key check keeps its (empty) scopes.
    function test_unmanagedUsageKeyAtAdminWalletHashKeepsItsScopes() public {
        vm.prank(apiPayer);
        writes.newAccount(UNMANAGED, false, "unmanaged", "", user);
        master = UNMANAGED;
        _seed(user, UNMANAGED);
        uint256 walletHash = apiKeyHashOf(user);
        uint256[] memory empty = new uint256[](0);
        vm.prank(user);
        writes.setUsageApiKey(
            UNMANAGED,
            walletHash,
            0,
            0,
            "unscoped",
            "",
            false,
            false,
            false,
            empty,
            empty,
            empty,
            empty
        );
        // Positive control: the same sender can make every write with the master hash.
        _assertActsAsMaster(UNMANAGED, user);
        _assertRejected(walletHash, user, "");
    }

    /// One admin's alias never reaches another account.
    function test_aliasOnlyReachesItsOwnAccount() public {
        _transferred();
        vm.prank(stranger);
        writes.newChainSecuredAccount("other", "");
        uint256 strangerHash = apiKeyHashOf(stranger);
        bytes32 strangerState = keccak256(
            abi.encode(
                views_.listGroups(strangerHash, 0, 100), views_.listActions(strangerHash, 0, 100)
            )
        );
        vm.prank(successor);
        writes.addAction(apiKeyHashOf(successor), "mine", "", NEW_ACTION);
        assertEq(views_.listActions(master, 0, 100).length, 2);
        assertEq(
            keccak256(
                abi.encode(
                    views_.listGroups(strangerHash, 0, 100),
                    views_.listActions(strangerHash, 0, 100)
                )
            ),
            strangerState
        );
        vm.prank(successor);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.NoAccountAccess.selector, strangerHash, successor)
        );
        writes.addAction(strangerHash, "theirs", "", NEW_ACTION);
    }
}

/// @notice Accounts converted or transferred BEFORE the WritesFacet upgrade
///         need no migration: the fix only reads the alias the old code
///         already wrote. State is written with the pre-upgrade facets, then
///         the facets are cut over the way the deployer does it.
/// @dev LegacyWritesFacet supplies the previous conversion and transfer behavior.
contract AdminWalletAliasUpgradeTest is Test {
    address internal owner = makeAddr("owner");
    address internal apiPayer = makeAddr("apiPayer");
    address internal alice = makeAddr("alice");
    address internal aliceSuccessor = makeAddr("aliceSuccessor");
    address internal convertedAdmin = makeAddr("convertedAdmin");
    uint256 internal constant MANAGED = uint256(keccak256("issue-147-upgrade-managed"));
    uint256 internal constant USAGE_A = uint256(keccak256("issue-147-upgrade-usage-a"));
    uint256 internal constant USAGE_M = uint256(keccak256("issue-147-upgrade-usage-m"));

    address payable internal diamond;

    function setUp() public {
        _deployLegacyDiamond();
        LegacyWritesFacet w = LegacyWritesFacet(diamond);
        uint256[] memory empty = new uint256[](0);

        vm.startPrank(alice);
        w.newChainSecuredAccount("alice", "");
        w.setUsageApiKey(
            _hashOf(alice),
            USAGE_A,
            0,
            0,
            "old",
            "",
            false,
            false,
            false,
            empty,
            empty,
            empty,
            empty
        );
        w.transferChainSecuredAccountOwnership(_hashOf(alice), aliceSuccessor);
        vm.stopPrank();

        vm.startPrank(apiPayer);
        w.newAccount(MANAGED, true, "managed", "", apiPayer);
        w.setUsageApiKey(
            MANAGED, USAGE_M, 0, 0, "old", "", false, false, false, empty, empty, empty, empty
        );
        w.convertToChainSecuredAccount(MANAGED, convertedAdmin);
        vm.stopPrank();
    }

    function test_aliasesWrittenBeforeTheUpgradeWorkAfterIt() public {
        uint256 successorAlias = _hashOf(aliceSuccessor);
        uint256 convertedAlias = _hashOf(convertedAdmin);
        uint256[] memory noCids = new uint256[](0);
        address[] memory noPkps = new address[](0);

        // Pre-upgrade code: the admin aliases are treated as usage keys (#147).
        vm.prank(aliceSuccessor);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.NotAllowedToCreateGroup.selector, successorAlias)
        );
        LegacyWritesFacet(diamond).addGroup(successorAlias, "g", "", noCids, noPkps);
        vm.prank(convertedAdmin);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.UsageApiKeyDoesNotExist.selector, convertedAlias, USAGE_M
            )
        );
        LegacyWritesFacet(diamond).removeUsageApiKey(convertedAlias, USAGE_M);

        // Views resolve the aliases to the master accounts on both sides of the upgrade.
        assertEq(ViewsFacet(diamond).listApiKeys(successorAlias, 0, 10).length, 1);
        assertEq(ViewsFacet(diamond).listApiKeys(convertedAlias, 0, 10).length, 1);

        _upgradeFacets();

        // Same storage, upgraded facet: the same calls succeed.
        vm.prank(aliceSuccessor);
        assertEq(WritesFacet(diamond).addGroup(successorAlias, "g", "", noCids, noPkps), 1);
        vm.prank(aliceSuccessor);
        WritesFacet(diamond).removeUsageApiKey(successorAlias, USAGE_A);
        vm.prank(convertedAdmin);
        WritesFacet(diamond).removeUsageApiKey(convertedAlias, USAGE_M);
        assertEq(ViewsFacet(diamond).listApiKeys(successorAlias, 0, 10).length, 0);
        assertEq(ViewsFacet(diamond).listApiKeys(convertedAlias, 0, 10).length, 0);
        assertEq(ViewsFacet(diamond).listGroups(_hashOf(alice), 0, 10).length, 1);

        // The previous admins stay locked out.
        vm.prank(alice);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.NoAccountAccess.selector, _hashOf(alice), alice)
        );
        WritesFacet(diamond).addGroup(_hashOf(alice), "x", "", noCids, noPkps);
        vm.prank(apiPayer);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.NoAccountAccess.selector, MANAGED, apiPayer)
        );
        WritesFacet(diamond).addGroup(MANAGED, "x", "", noCids, noPkps);
    }

    function _deployLegacyDiamond() internal {
        IDiamond.FacetCut[] memory cuts = new IDiamond.FacetCut[](7);
        cuts[0] = _cut(
            address(new DiamondCutFacet()),
            IDiamond.FacetCutAction.Add,
            DiamondDeploy.diamondCutSelectors()
        );
        cuts[1] = _cut(
            address(new DiamondLoupeFacet()),
            IDiamond.FacetCutAction.Add,
            DiamondDeploy.diamondLoupeSelectors()
        );
        cuts[2] = _cut(
            address(new OwnershipFacet()),
            IDiamond.FacetCutAction.Add,
            DiamondDeploy.ownershipSelectors()
        );
        cuts[3] = _cut(
            address(new LegacyViewsFacet()),
            IDiamond.FacetCutAction.Add,
            DiamondDeploy.viewsSelectors()
        );
        cuts[4] = _cut(
            address(new LegacyWritesFacet()),
            IDiamond.FacetCutAction.Add,
            DiamondDeploy.writesSelectors()
        );
        cuts[5] = _cut(
            address(new LegacyAPIConfigFacet()),
            IDiamond.FacetCutAction.Add,
            DiamondDeploy.apiConfigSelectors()
        );
        cuts[6] = _cut(
            address(new BillingFacet()),
            IDiamond.FacetCutAction.Add,
            DiamondDeploy.billingSelectors()
        );
        DiamondInit init = new DiamondInit();
        AccountConfig d = new AccountConfig(
            owner, cuts, address(init), abi.encodeWithSelector(DiamondInit.init.selector)
        );
        diamond = payable(address(d));

        address[] memory payers = new address[](1);
        payers[0] = apiPayer;
        vm.prank(owner);
        LegacyAPIConfigFacet(diamond).setApiPayers(payers);
    }

    /// Mirrors the production deployer: Replace every facet selector with the
    /// new facet address (this change adds no selectors).
    function _upgradeFacets() internal {
        address newWrites = address(new WritesFacet());
        IDiamond.FacetCut[] memory cuts = new IDiamond.FacetCut[](3);
        cuts[0] = _cut(
            address(new ViewsFacet()),
            IDiamond.FacetCutAction.Replace,
            DiamondDeploy.viewsSelectors()
        );
        cuts[1] = _cut(newWrites, IDiamond.FacetCutAction.Replace, DiamondDeploy.writesSelectors());
        cuts[2] = _cut(
            address(new APIConfigFacet()),
            IDiamond.FacetCutAction.Replace,
            DiamondDeploy.apiConfigSelectors()
        );
        vm.prank(owner);
        DiamondCutFacet(diamond).diamondCut(cuts, address(0), "");
        assertEq(DiamondLoupeFacet(diamond).facetAddress(WritesFacet.addGroup.selector), newWrites);
    }

    function _hashOf(address a) internal pure returns (uint256) {
        return uint256(keccak256(abi.encodePacked(a)));
    }

    function _cut(address facet, IDiamond.FacetCutAction action, bytes4[] memory sels)
        internal
        pure
        returns (IDiamond.FacetCut memory)
    {
        return IDiamond.FacetCut({facetAddress: facet, action: action, functionSelectors: sels});
    }
}
