// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {BaseTest} from "./helpers/BaseTest.sol";
import {DiamondDeploy} from "./helpers/DiamondDeploy.sol";
import {ViewsFacet} from "../contracts/AccountConfigFacets/ViewsFacet.sol";
import {WritesFacet} from "../contracts/AccountConfigFacets/WritesFacet.sol";
import {APIConfigFacet} from "../contracts/AccountConfigFacets/APIConfigFacet.sol";
import {BillingFacet} from "../contracts/AccountConfigFacets/BillingFacet.sol";
import {DiamondCutFacet} from "../libraries/diamond/DiamondCutFacet.sol";
import {DiamondLoupeFacet} from "../libraries/diamond/DiamondLoupeFacet.sol";
import {OwnershipFacet} from "../libraries/diamond/OwnershipFacet.sol";
import {IDiamondLoupe} from "../interfaces/IDiamondLoupe.sol";
import {IDiamondCut} from "../interfaces/IDiamondCut.sol";
import {IDiamond} from "../interfaces/IDiamond.sol";
import {IERC165} from "../interfaces/IERC165.sol";
import {IERC173} from "../interfaces/IERC173.sol";
import {NotContractOwner} from "../libraries/LibDiamond.sol";

contract DiamondTest is BaseTest {
    function test_owner_setAtConstruction() public view {
        assertEq(ownership.owner(), owner);
    }

    function test_loupe_facetsListsAllSevenFacets() public view {
        IDiamondLoupe.Facet[] memory facets = loupe.facets();
        assertEq(facets.length, 7, "expected 7 facets");

        // Every facet entry must reference a real, distinct address.
        for (uint256 i = 0; i < facets.length; i++) {
            assertTrue(facets[i].facetAddress != address(0));
            assertGt(facets[i].functionSelectors.length, 0);
        }
    }

    function test_loupe_facetAddressRoutesEachFacetSelector() public view {
        // Spot-check one selector per facet routes to the right deployed facet.
        assertEq(loupe.facetAddress(DiamondCutFacet.diamondCut.selector), d.diamondCut);
        assertEq(loupe.facetAddress(DiamondLoupeFacet.facets.selector), d.diamondLoupe);
        assertEq(loupe.facetAddress(OwnershipFacet.owner.selector), d.ownership);
        assertEq(loupe.facetAddress(ViewsFacet.accountCount.selector), d.views);
        assertEq(loupe.facetAddress(WritesFacet.newAccount.selector), d.writes);
        assertEq(loupe.facetAddress(APIConfigFacet.setApiPayers.selector), d.apiConfig);
        assertEq(loupe.facetAddress(BillingFacet.getPricing.selector), d.billing);
    }

    function test_loupe_unknownSelectorReturnsZero() public view {
        assertEq(loupe.facetAddress(bytes4(0xdeadbeef)), address(0));
    }

    function test_fallback_unknownSelectorReverts() public {
        // FunctionNotFound(bytes4) selector + arbitrary bytes4
        bytes4 unknown = bytes4(0xdeadbeef);
        (bool ok,) = d.diamond.call(abi.encodePacked(unknown));
        assertFalse(ok, "call to unknown selector should revert");
    }

    function test_directEthTransferReverts() public {
        vm.deal(stranger, 1 ether);
        vm.prank(stranger);
        (bool ok,) = d.diamond.call{value: 1}("");
        assertFalse(ok, "direct ETH transfer should revert");
    }

    function test_supportsInterface_standardIds() public view {
        assertTrue(loupe.supportsInterface(type(IERC165).interfaceId));
        assertTrue(loupe.supportsInterface(type(IDiamondCut).interfaceId));
        assertTrue(loupe.supportsInterface(type(IDiamondLoupe).interfaceId));
        assertTrue(loupe.supportsInterface(type(IERC173).interfaceId));
        assertFalse(loupe.supportsInterface(bytes4(0xffffffff)));
    }

    function test_transferOwnership_onlyOwner() public {
        vm.prank(stranger);
        vm.expectRevert(abi.encodeWithSelector(NotContractOwner.selector, stranger, owner));
        ownership.transferOwnership(stranger);
    }

    function test_transferOwnership_owner() public {
        vm.prank(owner);
        ownership.transferOwnership(user);
        assertEq(ownership.owner(), user);
    }

    function test_diamondCut_onlyOwner() public {
        IDiamond.FacetCut[] memory empty = new IDiamond.FacetCut[](0);
        vm.prank(stranger);
        vm.expectRevert(abi.encodeWithSelector(NotContractOwner.selector, stranger, owner));
        cut.diamondCut(empty, address(0), "");
    }

    function test_constructor_setsDefaults() public view {
        // Constructor primes pricing[1]=1, requestedApiPayerCount=3, pricingOperator=owner,
        // configOperator=owner.
        assertEq(views_.pricingOperator(), owner);
        assertEq(views_.configOperator(), owner);
        assertEq(billing.getPricing(1), 1);
        assertEq(views_.requestedApiPayerCount(), 3);
    }
}
