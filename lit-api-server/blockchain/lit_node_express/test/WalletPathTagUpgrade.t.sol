// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {Test} from "forge-std/Test.sol";
import {AccountConfig} from "../contracts/AccountConfig.sol";
import {AppStorage} from "../contracts/AccountConfigFacets/AppStorage.sol";
import {ViewsFacet} from "../contracts/AccountConfigFacets/ViewsFacet.sol";
import {WritesFacet} from "../contracts/AccountConfigFacets/WritesFacet.sol";
import {APIConfigFacet} from "../contracts/AccountConfigFacets/APIConfigFacet.sol";
import {BillingFacet} from "../contracts/AccountConfigFacets/BillingFacet.sol";
import {DiamondInit} from "../contracts/AccountConfigFacets/DiamondInit.sol";
import {DiamondCutFacet} from "../libraries/diamond/DiamondCutFacet.sol";
import {DiamondLoupeFacet} from "../libraries/diamond/DiamondLoupeFacet.sol";
import {OwnershipFacet} from "../libraries/diamond/OwnershipFacet.sol";
import {IDiamond} from "../interfaces/IDiamond.sol";
import {DiamondDeploy} from "./helpers/DiamondDeploy.sol";
import {LegacyViewsFacet} from "./legacy/LegacyViewsFacet.sol";
import {LegacyWritesFacet} from "./legacy/LegacyWritesFacet.sol";
import {LegacyAPIConfigFacet} from "./legacy/LegacyAPIConfigFacet.sol";

/// @notice Upgrade test for the wallet path tag (#140). State is written with
///         the PRE-UPGRADE facet code (untagged paths, exactly what the node
///         issued until now), the facets are then cut over the way the deployer
///         does it, and every pre-existing wallet must behave exactly as before.
///         Because the derivation path is unchanged, the node derives exactly
///         the same key for these wallets as before the upgrade.
contract WalletPathTagUpgradeTest is Test {
    address internal owner = makeAddr("owner");
    address internal apiPayer = makeAddr("apiPayer");
    address internal alice = makeAddr("alice");
    address internal bob = makeAddr("bob");
    address internal mallory = makeAddr("mallory");

    address payable internal diamond;

    // Pre-upgrade wallets, all with untagged (pre-tag) paths.
    address internal constant WALLET_PLAIN = address(0xA001); // registered only
    address internal constant WALLET_GROUPED = address(0xA002); // in a group with an action
    address internal constant WALLET_REMOVED = address(0xA003); // removed before the upgrade
    address internal constant WALLET_BOB = address(0xB001); // other account
    uint256 internal constant PATH_PLAIN = uint256(keccak256("path-plain"));
    uint256 internal constant PATH_GROUPED = uint256(keccak256("path-grouped"));
    uint256 internal constant PATH_REMOVED = uint256(keccak256("path-removed"));
    uint256 internal constant PATH_BOB = uint256(keccak256("path-bob"));
    uint256 internal constant ACTION = uint256(keccak256("QmLegacyAction"));

    uint256 internal aliceHash;
    uint256 internal bobHash;
    uint256 internal malloryHash;

    function setUp() public {
        aliceHash = _hashOf(alice);
        bobHash = _hashOf(bob);
        malloryHash = _hashOf(mallory);
        _deployLegacyDiamond();
        _seedWithLegacyFacets();
    }

    // ---------------------------------------------------------------------
    // Old wallets keep working
    // ---------------------------------------------------------------------

    function test_oldWalletsResolveIdenticallyAfterUpgrade() public {
        Snapshot memory beforeUpgrade = _snapshot();
        _upgradeFacets();
        _assertSnapshotEq(beforeUpgrade, _snapshot());
    }

    function test_oldWalletsKeepWorkingThroughPostUpgradeWrites() public {
        _upgradeFacets();

        // Add an old wallet to a new group.
        uint256[] memory cids = new uint256[](1);
        cids[0] = ACTION;
        address[] memory noPkps = new address[](0);
        vm.prank(alice);
        uint256 g2 = WritesFacet(diamond).addGroup(aliceHash, "g2", "g2", cids, noPkps);
        vm.prank(alice);
        WritesFacet(diamond).addPkpToGroup(aliceHash, g2, WALLET_PLAIN);
        assertEq(
            ViewsFacet(diamond).groupIdsForActionAndWallet(aliceHash, ACTION, WALLET_PLAIN).length,
            1
        );
        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, WALLET_PLAIN), PATH_PLAIN);

        // Recovery flow on a wallet removed BEFORE the upgrade: its untagged
        // path is already bound to Alice, so re-registering it still works.
        vm.prank(alice);
        WritesFacet(diamond)
            .registerWalletDerivation(aliceHash, WALLET_REMOVED, PATH_REMOVED, "restored", "");
        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, WALLET_REMOVED), PATH_REMOVED);

        // Remove + re-register of a live old wallet after the upgrade.
        vm.prank(alice);
        WritesFacet(diamond).removeWalletDerivation(aliceHash, WALLET_GROUPED);
        vm.prank(alice);
        WritesFacet(diamond)
            .registerWalletDerivation(aliceHash, WALLET_GROUPED, PATH_GROUPED, "again", "");
        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, WALLET_GROUPED), PATH_GROUPED);

        // Bob's old wallet is untouched by all of this.
        assertEq(ViewsFacet(diamond).getWalletDerivation(bobHash, WALLET_BOB), PATH_BOB);
    }

    function test_newWalletsNeedNodeIssuedPathsAfterUpgrade() public {
        _upgradeFacets();

        // A node-issued (tagged) path registers normally.
        uint256 tagged = _tagged(uint256(keccak256("fresh")));
        vm.prank(alice);
        WritesFacet(diamond).registerWalletDerivation(aliceHash, address(0xA0FF), tagged, "new", "");
        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, address(0xA0FF)), tagged);

        // A fresh untagged path is rejected.
        vm.prank(alice);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector, "invalid wallet derivation path tag"
            )
        );
        WritesFacet(diamond)
            .registerWalletDerivation(aliceHash, address(0xA0FE), uint256(keccak256("x")), "x", "");
    }

    /// #140 after the upgrade: registering a Lit Action's KMS path as a wallet
    /// (directly, or by laundering the action's address through a tagged
    /// registration first) is rejected.
    function test_actionPathCannotBeRegisteredAfterUpgrade() public {
        _upgradeFacets();
        address actionWallet = makeAddr("actionWallet");
        uint256 actionPath = uint256(keccak256("lit_action_QmVictimAction"));

        vm.prank(mallory);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector, "invalid wallet derivation path tag"
            )
        );
        WritesFacet(diamond)
            .registerWalletDerivation(malloryHash, actionWallet, actionPath, "a", "");

        // Bind the action's address with a legit tagged path, delete it, then
        // try to re-register the address with the action's path.
        vm.prank(mallory);
        WritesFacet(diamond)
            .registerWalletDerivation(malloryHash, actionWallet, _tagged(7), "a", "");
        vm.prank(mallory);
        WritesFacet(diamond).removeWalletDerivation(malloryHash, actionWallet);
        vm.prank(mallory);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector, "invalid wallet derivation path tag"
            )
        );
        WritesFacet(diamond)
            .registerWalletDerivation(malloryHash, actionWallet, actionPath, "a", "");
    }

    function test_preExistingOwnershipChecksStillApplyAfterUpgrade() public {
        _upgradeFacets();

        // Bob has no entry for Alice's old wallet.
        assertEq(ViewsFacet(diamond).getWalletDerivation(bobHash, WALLET_PLAIN), 0);

        // Bob cannot take over Alice's old pkpId...
        vm.prank(bob);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector, "PKP owned by another account"
            )
        );
        WritesFacet(diamond).registerWalletDerivation(bobHash, WALLET_PLAIN, _tagged(1), "x", "");

        // ...nor alias her old (untagged, already bound) path.
        vm.prank(bob);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector, "derivation path owned by another account"
            )
        );
        WritesFacet(diamond).registerWalletDerivation(bobHash, address(0xB0FF), PATH_PLAIN, "x", "");

        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, WALLET_PLAIN), PATH_PLAIN);
    }

    /// Rollout order: api-servers are upgraded first and start issuing tagged
    /// paths while the contract still runs the old code. Those registrations
    /// must keep working across the contract upgrade.
    function test_taggedWalletRegisteredBeforeUpgradeKeepsWorking() public {
        uint256 early = _tagged(uint256(keccak256("issued-by-new-node-before-upgrade")));
        vm.prank(alice);
        LegacyWritesFacet(diamond).registerWalletDerivation(aliceHash, address(0xA0EE), early, "early", "");
        _upgradeFacets();
        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, address(0xA0EE)), early);
        vm.prank(alice);
        WritesFacet(diamond).removeWalletDerivation(aliceHash, address(0xA0EE));
        vm.prank(alice);
        WritesFacet(diamond).registerWalletDerivation(aliceHash, address(0xA0EE), early, "early", "");
        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, address(0xA0EE)), early);
    }

    /// Straddler: an OLD node issued an untagged path before the upgrade, and
    /// the client registers it only after the facet cut. That registration
    /// reverts loudly with the tag error (no wallet is left half-created), and
    /// minting again from an upgraded node (what the SDK's createWallet does on
    /// this error) succeeds.
    function test_untaggedPathIssuedBeforeUpgradeRegisteredAfterRevertsAndRemintWorks() public {
        uint256 issuedByOldNode = uint256(keccak256("issued-by-old-node"));
        _upgradeFacets();
        vm.prank(alice);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.InvalidRequest.selector, "invalid wallet derivation path tag")
        );
        WritesFacet(diamond).registerWalletDerivation(aliceHash, address(0xA0DD), issuedByOldNode, "w", "");
        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, address(0xA0DD)), 0);
        assertEq(ViewsFacet(diamond).getPkpOwnerMaster(address(0xA0DD)), 0, "nothing bound by the revert");

        uint256 reminted = _tagged(uint256(keccak256("re-minted-by-upgraded-node")));
        vm.prank(alice);
        WritesFacet(diamond).registerWalletDerivation(aliceHash, address(0xA0DC), reminted, "w", "");
        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, address(0xA0DC)), reminted);
    }

    /// The api-server mints paths with the same tag (pinned on both sides).
    function test_tagConstantIsPinned() public pure {
        assertEq(AppStorage.WALLET_PATH_TAG, 0x6c69742d77616c6c65743a31); // "lit-wallet:1"
        assertEq(AppStorage.WALLET_PATH_TAG_SHIFT, 160);
    }

    // ---------------------------------------------------------------------
    // Snapshot helpers (only selectors present on both legacy and new facets)
    // ---------------------------------------------------------------------

    struct Snapshot {
        uint256 plain;
        uint256 grouped;
        uint256 removed;
        uint256 bobPath;
        uint256 alicePkpCount;
        uint256 groupedGroups;
        uint256 plainGroups;
        uint256 ownerPlain;
        uint256 ownerRemoved;
        uint256 pathOwnerPlain;
        uint256 pathOwnerRemoved;
    }

    function _snapshot() internal view returns (Snapshot memory s) {
        LegacyViewsFacet v = LegacyViewsFacet(diamond);
        s.plain = v.getWalletDerivation(aliceHash, WALLET_PLAIN);
        s.grouped = v.getWalletDerivation(aliceHash, WALLET_GROUPED);
        s.removed = v.getWalletDerivation(aliceHash, WALLET_REMOVED);
        s.bobPath = v.getWalletDerivation(bobHash, WALLET_BOB);
        s.alicePkpCount = v.listPkps(aliceHash, 0, 100).length;
        s.groupedGroups = v.groupIdsForActionAndWallet(aliceHash, ACTION, WALLET_GROUPED).length;
        s.plainGroups = v.groupIdsForActionAndWallet(aliceHash, ACTION, WALLET_PLAIN).length;
        s.ownerPlain = v.getPkpOwnerMaster(WALLET_PLAIN);
        s.ownerRemoved = v.getPkpOwnerMaster(WALLET_REMOVED);
        s.pathOwnerPlain = v.getPathOwnerMaster(PATH_PLAIN);
        s.pathOwnerRemoved = v.getPathOwnerMaster(PATH_REMOVED);
    }

    function _assertSnapshotEq(Snapshot memory a, Snapshot memory b) internal view {
        assertEq(b.plain, a.plain, "plain path");
        assertEq(b.grouped, a.grouped, "grouped path");
        assertEq(b.removed, a.removed, "removed path");
        assertEq(b.bobPath, a.bobPath, "bob path");
        assertEq(b.alicePkpCount, a.alicePkpCount, "pkp count");
        assertEq(b.groupedGroups, a.groupedGroups, "grouped membership");
        assertEq(b.plainGroups, a.plainGroups, "plain membership");
        assertEq(b.ownerPlain, a.ownerPlain, "owner plain");
        assertEq(b.ownerRemoved, a.ownerRemoved, "owner removed");
        assertEq(b.pathOwnerPlain, a.pathOwnerPlain, "path owner plain");
        assertEq(b.pathOwnerRemoved, a.pathOwnerRemoved, "path owner removed");
        // Sanity: the seeded state is what we think it is.
        assertEq(a.plain, PATH_PLAIN);
        assertEq(a.grouped, PATH_GROUPED);
        assertEq(a.removed, 0);
        assertEq(a.bobPath, PATH_BOB);
        assertEq(a.alicePkpCount, 2);
        assertEq(a.groupedGroups, 1);
        assertEq(a.plainGroups, 0);
        assertEq(a.ownerPlain, aliceHash);
        assertEq(a.ownerRemoved, aliceHash);
        assertEq(a.pathOwnerPlain, aliceHash);
        assertEq(a.pathOwnerRemoved, aliceHash);
        // The seeded paths really are pre-tag (untagged) paths.
        assertTrue(PATH_PLAIN >> AppStorage.WALLET_PATH_TAG_SHIFT != AppStorage.WALLET_PATH_TAG);
        assertTrue(PATH_REMOVED >> AppStorage.WALLET_PATH_TAG_SHIFT != AppStorage.WALLET_PATH_TAG);
    }

    // ---------------------------------------------------------------------
    // Deployment: pre-upgrade diamond, seeding, and the facet upgrade
    // ---------------------------------------------------------------------

    function _deployLegacyDiamond() internal {
        IDiamond.FacetCut[] memory cut = new IDiamond.FacetCut[](7);
        cut[0] = _add(address(new DiamondCutFacet()), DiamondDeploy.diamondCutSelectors());
        cut[1] = _add(address(new DiamondLoupeFacet()), DiamondDeploy.diamondLoupeSelectors());
        cut[2] = _add(address(new OwnershipFacet()), DiamondDeploy.ownershipSelectors());
        cut[3] = _add(address(new LegacyViewsFacet()), DiamondDeploy.viewsSelectors());
        cut[4] = _add(address(new LegacyWritesFacet()), DiamondDeploy.writesSelectors());
        cut[5] = _add(address(new LegacyAPIConfigFacet()), DiamondDeploy.apiConfigSelectors());
        cut[6] = _add(address(new BillingFacet()), DiamondDeploy.billingSelectors());
        DiamondInit init = new DiamondInit();
        AccountConfig d = new AccountConfig(
            owner, cut, address(init), abi.encodeWithSelector(DiamondInit.init.selector)
        );
        diamond = payable(address(d));

        address[] memory payers = new address[](1);
        payers[0] = apiPayer;
        vm.prank(owner);
        LegacyAPIConfigFacet(diamond).setApiPayers(payers);
    }

    function _seedWithLegacyFacets() internal {
        LegacyWritesFacet w = LegacyWritesFacet(diamond);
        vm.prank(alice);
        w.newChainSecuredAccount("alice", "alice");
        vm.prank(bob);
        w.newChainSecuredAccount("bob", "bob");

        vm.startPrank(alice);
        w.registerWalletDerivation(aliceHash, WALLET_PLAIN, PATH_PLAIN, "plain", "plain");
        w.registerWalletDerivation(aliceHash, WALLET_GROUPED, PATH_GROUPED, "grp", "grp");
        w.registerWalletDerivation(aliceHash, WALLET_REMOVED, PATH_REMOVED, "rm", "rm");
        w.removeWalletDerivation(aliceHash, WALLET_REMOVED);
        w.addAction(aliceHash, "action", "action", ACTION);
        uint256[] memory cids = new uint256[](1);
        cids[0] = ACTION;
        address[] memory pkps = new address[](1);
        pkps[0] = WALLET_GROUPED;
        w.addGroup(aliceHash, "g", "g", cids, pkps);
        vm.stopPrank();

        vm.prank(bob);
        w.registerWalletDerivation(bobHash, WALLET_BOB, PATH_BOB, "bob", "bob");

        // The pre-upgrade code accepts untagged fresh paths (this is the bug).
        vm.prank(mallory);
        w.newChainSecuredAccount("m0", "m0");
        vm.prank(mallory);
        w.registerWalletDerivation(
            malloryHash, address(0xC0DE), uint256(keccak256("untagged")), "m", ""
        );
    }

    /// Mirrors the production deployer: Replace every facet selector with the
    /// new facet address (this change adds no selectors).
    function _upgradeFacets() internal {
        address newViews = address(new ViewsFacet());
        address newWrites = address(new WritesFacet());
        address newApi = address(new APIConfigFacet());
        IDiamond.FacetCut[] memory cut = new IDiamond.FacetCut[](3);
        cut[0] = _cut(newViews, IDiamond.FacetCutAction.Replace, DiamondDeploy.viewsSelectors());
        cut[1] = _cut(newWrites, IDiamond.FacetCutAction.Replace, DiamondDeploy.writesSelectors());
        cut[2] = _cut(newApi, IDiamond.FacetCutAction.Replace, DiamondDeploy.apiConfigSelectors());
        vm.prank(owner);
        DiamondCutFacet(diamond).diamondCut(cut, address(0), "");

        assertEq(
            DiamondLoupeFacet(diamond).facetAddress(WritesFacet.registerWalletDerivation.selector),
            newWrites
        );
        assertEq(
            DiamondLoupeFacet(diamond).facetAddress(ViewsFacet.getWalletDerivation.selector),
            newViews
        );
    }

    // ---------------------------------------------------------------------
    // Small helpers
    // ---------------------------------------------------------------------

    function _hashOf(address a) internal pure returns (uint256) {
        return uint256(keccak256(abi.encodePacked(a)));
    }

    function _tagged(uint256 n) internal pure returns (uint256) {
        return
            (AppStorage.WALLET_PATH_TAG << AppStorage.WALLET_PATH_TAG_SHIFT)
                | (n & type(uint160).max);
    }

    function _add(address facet, bytes4[] memory sels)
        internal
        pure
        returns (IDiamond.FacetCut memory)
    {
        return _cut(facet, IDiamond.FacetCutAction.Add, sels);
    }

    function _cut(address facet, IDiamond.FacetCutAction action, bytes4[] memory sels)
        internal
        pure
        returns (IDiamond.FacetCut memory)
    {
        return IDiamond.FacetCut({facetAddress: facet, action: action, functionSelectors: sels});
    }
}
