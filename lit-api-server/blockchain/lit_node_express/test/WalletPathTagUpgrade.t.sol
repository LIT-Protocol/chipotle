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

contract WalletPathTagUpgradeTest is Test {
    address internal owner = makeAddr("owner");
    address internal apiPayer = makeAddr("apiPayer");
    address internal alice = makeAddr("alice");
    address internal bob = makeAddr("bob");
    address internal mallory = makeAddr("mallory");

    address payable internal diamond;

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


    function test_oldWalletsResolveIdenticallyAfterUpgrade() public {
        Snapshot memory beforeUpgrade = _snapshot();
        _upgradeFacets();
        _assertSnapshotEq(beforeUpgrade, _snapshot());
    }

    function test_oldWalletsKeepWorkingThroughPostUpgradeWrites() public {
        _upgradeFacets();

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

        vm.prank(alice);
        WritesFacet(diamond)
            .registerWalletDerivation(aliceHash, WALLET_REMOVED, PATH_REMOVED, "restored", "");
        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, WALLET_REMOVED), PATH_REMOVED);

        vm.prank(alice);
        WritesFacet(diamond).removeWalletDerivation(aliceHash, WALLET_GROUPED);
        vm.prank(alice);
        WritesFacet(diamond)
            .registerWalletDerivation(aliceHash, WALLET_GROUPED, PATH_GROUPED, "again", "");
        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, WALLET_GROUPED), PATH_GROUPED);

        assertEq(ViewsFacet(diamond).getWalletDerivation(bobHash, WALLET_BOB), PATH_BOB);
    }

    function test_newWalletsNeedNodeIssuedPathsAfterUpgrade() public {
        _upgradeFacets();

        uint256 tagged = _tagged(uint256(keccak256("fresh")));
        vm.prank(alice);
        WritesFacet(diamond).registerWalletDerivation(aliceHash, address(0xA0FF), tagged, "new", "");
        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, address(0xA0FF)), tagged);

        vm.prank(alice);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector, "invalid wallet derivation path tag"
            )
        );
        WritesFacet(diamond)
            .registerWalletDerivation(aliceHash, address(0xA0FE), uint256(keccak256("x")), "x", "");
    }

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

        assertEq(ViewsFacet(diamond).getWalletDerivation(bobHash, WALLET_PLAIN), 0);

        vm.prank(bob);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector, "PKP owned by another account"
            )
        );
        WritesFacet(diamond).registerWalletDerivation(bobHash, WALLET_PLAIN, _tagged(1), "x", "");

        vm.prank(bob);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector, "derivation path owned by another account"
            )
        );
        WritesFacet(diamond).registerWalletDerivation(bobHash, address(0xB0FF), PATH_PLAIN, "x", "");

        assertEq(ViewsFacet(diamond).getWalletDerivation(aliceHash, WALLET_PLAIN), PATH_PLAIN);
    }

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

    function test_tagConstantIsPinned() public pure {
        assertEq(AppStorage.WALLET_PATH_TAG, 0x6c69742d77616c6c65743a31); // "lit-wallet:1"
        assertEq(AppStorage.WALLET_PATH_TAG_SHIFT, 160);
    }


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
        assertTrue(PATH_PLAIN >> AppStorage.WALLET_PATH_TAG_SHIFT != AppStorage.WALLET_PATH_TAG);
        assertTrue(PATH_REMOVED >> AppStorage.WALLET_PATH_TAG_SHIFT != AppStorage.WALLET_PATH_TAG);
    }


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

        vm.prank(mallory);
        w.newChainSecuredAccount("m0", "m0");
        vm.prank(mallory);
        w.registerWalletDerivation(
            malloryHash, address(0xC0DE), uint256(keccak256("untagged")), "m", ""
        );
    }

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
