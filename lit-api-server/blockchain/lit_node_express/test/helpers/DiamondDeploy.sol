// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {AccountConfig} from "../../contracts/AccountConfig.sol";
import {ViewsFacet} from "../../contracts/AccountConfigFacets/ViewsFacet.sol";
import {WritesFacet} from "../../contracts/AccountConfigFacets/WritesFacet.sol";
import {APIConfigFacet} from "../../contracts/AccountConfigFacets/APIConfigFacet.sol";
import {BillingFacet} from "../../contracts/AccountConfigFacets/BillingFacet.sol";
import {DiamondInit} from "../../contracts/AccountConfigFacets/DiamondInit.sol";
import {DiamondCutFacet} from "../../libraries/diamond/DiamondCutFacet.sol";
import {DiamondLoupeFacet} from "../../libraries/diamond/DiamondLoupeFacet.sol";
import {OwnershipFacet} from "../../libraries/diamond/OwnershipFacet.sol";
import {IDiamond} from "../../interfaces/IDiamond.sol";
import {IDiamondCut} from "../../interfaces/IDiamondCut.sol";

/// @notice Test-only helper that deploys an `AccountConfig` diamond with all
///         production facets cut in.
/// @dev    The production Rust `contract_deployer` derives selectors from the
///         compiled facet ABI. To keep this helper self-contained (and avoid
///         pulling in cheatcode-driven ABI parsing in every test run), the
///         selector arrays below are maintained by hand. If you add or rename
///         a public/external function on any facet, update the matching
///         `*Selectors()` function or the new selector will not be cut in.
library DiamondDeploy {
    struct Deployed {
        address payable diamond;
        address diamondCut;
        address diamondLoupe;
        address ownership;
        address views;
        address writes;
        address apiConfig;
        address billing;
        address diamondInit;
    }

    function deploy(address owner) internal returns (Deployed memory d) {
        d.diamondCut = address(new DiamondCutFacet());
        d.diamondLoupe = address(new DiamondLoupeFacet());
        d.ownership = address(new OwnershipFacet());
        d.views = address(new ViewsFacet());
        d.writes = address(new WritesFacet());
        d.apiConfig = address(new APIConfigFacet());
        d.billing = address(new BillingFacet());
        d.diamondInit = address(new DiamondInit());

        IDiamond.FacetCut[] memory cut = new IDiamond.FacetCut[](7);
        cut[0] = IDiamond.FacetCut({
            facetAddress: d.diamondCut,
            action: IDiamond.FacetCutAction.Add,
            functionSelectors: diamondCutSelectors()
        });
        cut[1] = IDiamond.FacetCut({
            facetAddress: d.diamondLoupe,
            action: IDiamond.FacetCutAction.Add,
            functionSelectors: diamondLoupeSelectors()
        });
        cut[2] = IDiamond.FacetCut({
            facetAddress: d.ownership,
            action: IDiamond.FacetCutAction.Add,
            functionSelectors: ownershipSelectors()
        });
        cut[3] = IDiamond.FacetCut({
            facetAddress: d.views,
            action: IDiamond.FacetCutAction.Add,
            functionSelectors: viewsSelectors()
        });
        cut[4] = IDiamond.FacetCut({
            facetAddress: d.writes,
            action: IDiamond.FacetCutAction.Add,
            functionSelectors: writesSelectors()
        });
        cut[5] = IDiamond.FacetCut({
            facetAddress: d.apiConfig,
            action: IDiamond.FacetCutAction.Add,
            functionSelectors: apiConfigSelectors()
        });
        cut[6] = IDiamond.FacetCut({
            facetAddress: d.billing,
            action: IDiamond.FacetCutAction.Add,
            functionSelectors: billingSelectors()
        });

        bytes memory initCalldata = abi.encodeWithSelector(DiamondInit.init.selector);
        AccountConfig diamond = new AccountConfig(owner, cut, d.diamondInit, initCalldata);
        d.diamond = payable(address(diamond));
    }

    function diamondCutSelectors() internal pure returns (bytes4[] memory s) {
        s = new bytes4[](1);
        s[0] = DiamondCutFacet.diamondCut.selector;
    }

    function diamondLoupeSelectors() internal pure returns (bytes4[] memory s) {
        s = new bytes4[](5);
        s[0] = DiamondLoupeFacet.facets.selector;
        s[1] = DiamondLoupeFacet.facetFunctionSelectors.selector;
        s[2] = DiamondLoupeFacet.facetAddresses.selector;
        s[3] = DiamondLoupeFacet.facetAddress.selector;
        s[4] = DiamondLoupeFacet.supportsInterface.selector;
    }

    function ownershipSelectors() internal pure returns (bytes4[] memory s) {
        s = new bytes4[](2);
        s[0] = OwnershipFacet.transferOwnership.selector;
        s[1] = OwnershipFacet.owner.selector;
    }

    function viewsSelectors() internal pure returns (bytes4[] memory s) {
        s = new bytes4[](35);
        s[0] = ViewsFacet.adminApiPayerAccount.selector;
        s[1] = ViewsFacet.api_payers.selector;
        s[2] = ViewsFacet.pricingOperator.selector;
        s[3] = ViewsFacet.configOperator.selector;
        s[4] = ViewsFacet.pkpCount.selector;
        s[5] = ViewsFacet.accountCount.selector;
        s[6] = ViewsFacet.indexToAccountHashAt.selector;
        s[7] = ViewsFacet.allPkpIdsAt.selector;
        s[8] = ViewsFacet.pricingAt.selector;
        s[9] = ViewsFacet.apiPayerCount.selector;
        s[10] = ViewsFacet.requestedApiPayerCount.selector;
        s[11] = ViewsFacet.rebalanceAmount.selector;
        s[12] = ViewsFacet.nodeConfigurationKeys.selector;
        s[13] = ViewsFacet.nodeConfigurationValue.selector;
        s[14] = ViewsFacet.nodeConfigurationValues.selector;
        s[15] = ViewsFacet.accountExistsAndIsMutable.selector;
        s[16] = ViewsFacet.getAccountWalletAddress.selector;
        s[17] = ViewsFacet.getBillingWalletAddress.selector;
        s[18] = ViewsFacet.getWalletDerivation.selector;
        s[19] = ViewsFacet.listApiKeys.selector;
        s[20] = ViewsFacet.listGroups.selector;
        s[21] = ViewsFacet.listGroupContents.selector;
        s[22] = ViewsFacet.listPkps.selector;
        s[23] = ViewsFacet.listWalletsInGroup.selector;
        s[24] = ViewsFacet.listActions.selector;
        s[25] = ViewsFacet.listActionsInGroup.selector;
        s[26] = ViewsFacet.canExecuteAction.selector;
        s[27] = ViewsFacet.canUseWalletInAction.selector;
        s[28] = ViewsFacet.apiKeyCanExecuteForAnyGroup.selector;
        s[29] = ViewsFacet.groupIdsForAction.selector;
        s[30] = ViewsFacet.groupIdsForActionAndWallet.selector;
        s[31] = ViewsFacet.canExecuteActionFast.selector;
        s[32] = ViewsFacet.canUseWalletInActionFast.selector;
        s[33] = ViewsFacet.canExecuteActionAndUseWallet.selector;
        s[34] = ViewsFacet.getPkpOwnerMaster.selector;
    }

    function writesSelectors() internal pure returns (bytes4[] memory s) {
        s = new bytes4[](20);
        s[0] = WritesFacet.newChainSecuredAccount.selector;
        s[1] = WritesFacet.newAccount.selector;
        s[2] = WritesFacet.convertToChainSecuredAccount.selector;
        s[3] = WritesFacet.setUsageApiKey.selector;
        s[4] = WritesFacet.addGroup.selector;
        s[5] = WritesFacet.updateGroup.selector;
        s[6] = WritesFacet.updateGroupMetadata.selector;
        s[7] = WritesFacet.removeGroup.selector;
        s[8] = WritesFacet.addPkpToGroup.selector;
        s[9] = WritesFacet.addAction.selector;
        s[10] = WritesFacet.removeAction.selector;
        s[11] = WritesFacet.addActionToGroup.selector;
        s[12] = WritesFacet.updateActionMetadata.selector;
        s[13] = WritesFacet.removeActionFromGroup.selector;
        s[14] = WritesFacet.removePkpFromGroup.selector;
        s[15] = WritesFacet.updateUsageApiKeyMetadata.selector;
        s[16] = WritesFacet.removeUsageApiKey.selector;
        s[17] = WritesFacet.registerWalletDerivation.selector;
        s[18] = WritesFacet.setNodeConfiguration.selector;
        s[19] = WritesFacet.backfillPkpOwners.selector;
    }

    function apiConfigSelectors() internal pure returns (bytes4[] memory s) {
        s = new bytes4[](6);
        s[0] = APIConfigFacet.setConfigOperator.selector;
        s[1] = APIConfigFacet.setRequestedApiPayerCount.selector;
        s[2] = APIConfigFacet.setAdminApiPayerAccount.selector;
        s[3] = APIConfigFacet.setApiPayers.selector;
        s[4] = APIConfigFacet.setRebalanceAmount.selector;
        s[5] = APIConfigFacet.serverTrigger.selector;
    }

    function billingSelectors() internal pure returns (bytes4[] memory s) {
        s = new bytes4[](5);
        s[0] = BillingFacet.getPricing.selector;
        s[1] = BillingFacet.debitApiKey.selector;
        s[2] = BillingFacet.creditApiKey.selector;
        s[3] = BillingFacet.setPricing.selector;
        s[4] = BillingFacet.setPricingOperator.selector;
    }
}
