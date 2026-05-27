// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {Test} from "forge-std/Test.sol";
import {DiamondDeploy} from "./DiamondDeploy.sol";
import {ViewsFacet} from "../../contracts/AccountConfigFacets/ViewsFacet.sol";
import {WritesFacet} from "../../contracts/AccountConfigFacets/WritesFacet.sol";
import {APIConfigFacet} from "../../contracts/AccountConfigFacets/APIConfigFacet.sol";
import {BillingFacet} from "../../contracts/AccountConfigFacets/BillingFacet.sol";
import {DiamondCutFacet} from "../../libraries/diamond/DiamondCutFacet.sol";
import {DiamondLoupeFacet} from "../../libraries/diamond/DiamondLoupeFacet.sol";
import {OwnershipFacet} from "../../libraries/diamond/OwnershipFacet.sol";

/// @notice Base for AccountConfig diamond tests. Deploys the full diamond and
///         exposes facet-typed views into the same address so individual tests
///         stay readable.
abstract contract BaseTest is Test {
    address internal owner = makeAddr("owner");
    address internal apiPayer = makeAddr("apiPayer");
    address internal adminApiPayer = makeAddr("adminApiPayer");
    address internal user = makeAddr("user");
    address internal stranger = makeAddr("stranger");

    DiamondDeploy.Deployed internal d;

    // Facet-typed views into the diamond address.
    ViewsFacet internal views_;
    WritesFacet internal writes;
    APIConfigFacet internal apiConfig;
    BillingFacet internal billing;
    DiamondCutFacet internal cut;
    DiamondLoupeFacet internal loupe;
    OwnershipFacet internal ownership;

    function setUp() public virtual {
        d = DiamondDeploy.deploy(owner);
        views_ = ViewsFacet(d.diamond);
        writes = WritesFacet(d.diamond);
        apiConfig = APIConfigFacet(d.diamond);
        billing = BillingFacet(d.diamond);
        cut = DiamondCutFacet(d.diamond);
        loupe = DiamondLoupeFacet(d.diamond);
        ownership = OwnershipFacet(d.diamond);

        // Wire the api_payer set + admin api payer the same way production does.
        address[] memory payers = new address[](1);
        payers[0] = apiPayer;
        vm.prank(owner);
        apiConfig.setApiPayers(payers);
        vm.prank(owner);
        apiConfig.setAdminApiPayerAccount(adminApiPayer);
    }

    function apiKeyHashOf(address wallet) internal pure returns (uint256) {
        return uint256(keccak256(abi.encodePacked(wallet)));
    }
}
