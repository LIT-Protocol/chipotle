/// @title AccountConfigWrite
/// @author Brendon Paul
/// @notice Security functions for AccountConfig diamond.

// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {AppStorage} from "./AppStorage.sol";
import {LibDiamond, NotContractOwner} from "../../libraries/LibDiamond.sol";

library SecurityLib {
    using EnumerableSet for EnumerableSet.UintSet;
    using EnumerableSet for EnumerableSet.AddressSet;

    // ----- internal view helpers (revert helpers call view from Views facet conceptually; we duplicate for simplicity) -----
    function revertIfNotOwner(address caller) internal view {
        if (caller != LibDiamond.contractOwner()) {
            revert NotContractOwner(caller, LibDiamond.contractOwner());
        }
    }

    function revertIfNotOwnerOrAdminApiPayer(address caller) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (
            caller != LibDiamond.contractOwner() &&
            caller != s.adminApiPayerAccount
        ) {
            revert AppStorage.OnlyApiPayerOrOwner(caller);
        }
    }

    function revertIfNotMasterAccount(uint256 accountApiKeyHash) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (s.allApiKeyHashesToMaster[accountApiKeyHash] != accountApiKeyHash) {
            revert AppStorage.NotMasterAccount(accountApiKeyHash);
        }
    }

    function revertIfNoAccountAccess(
        uint256 accountApiKeyHash,
        address sender
    ) internal view {
        AppStorage.revertIfNoAccountAccess(accountApiKeyHash, sender);
    }

    function revertIfGroupDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId,
        address sender
    ) internal view {
        revertIfNoAccountAccess(accountApiKeyHash, sender);
        AppStorage.revertIfGroupDoesNotExist(accountApiKeyHash, groupId);
    }

    function revertIfActionDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 action,
        address sender
    ) internal view {
        revertIfGroupDoesNotExist(accountApiKeyHash, groupId, sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (
            !s.accounts[accountApiKeyHash].groups[groupId].cidHash.contains(
                action
            )
        ) {
            revert AppStorage.ActionDoesNotExist(
                accountApiKeyHash,
                groupId,
                action
            );
        }
    }

    function revertIfPkpDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId,
        address pkpId,
        address sender
    ) internal view {
        revertIfGroupDoesNotExist(accountApiKeyHash, groupId, sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (
            !s.accounts[accountApiKeyHash].groups[groupId].pkpId.contains(pkpId)
        ) {
            revert AppStorage.PkpDoesNotExist(
                accountApiKeyHash,
                groupId,
                pkpId
            );
        }
    }

    function revertIfUsageApiKeyDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash
    ) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (
            s
                .accounts[accountApiKeyHash]
                .usageApiKeys[usageApiKeyHash]
                .apiKeyHash != usageApiKeyHash
        ) {
            revert AppStorage.UsageApiKeyDoesNotExist(
                accountApiKeyHash,
                usageApiKeyHash
            );
        }
    }

    function revertIfNotApiPayerOrPricingOperator(
        address caller
    ) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (!s.api_payers.contains(caller) && caller != s.pricingOperator) {
            revert AppStorage.OnlyApiPayerOrPricingOperator(caller);
        }
    }
    function revertIfNotConfigOperatorOrOwner(address caller) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (caller != s.configOperator && caller != LibDiamond.contractOwner()) {
            revert AppStorage.OnlyConfigOperatorOrOwner(caller);
        }
    }

    function revertIfNotApiPayerOrOwner(address caller) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (
            !s.api_payers.contains(caller) &&
            caller != LibDiamond.contractOwner() &&
            caller != s.adminApiPayerAccount
        ) {
            revert AppStorage.OnlyApiPayerOrOwner(caller);
        }
    }

    function checkIfApiPayerOrPricingOperator(address caller) internal view {
        AppStorage.checkIfApiPayerOrPricingOperator(
            AppStorage.getStorage(),
            caller
        );
    }

    function checkIfApiPayer(address caller) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (!s.api_payers.contains(caller)) {
            revert AppStorage.OnlyApiPayer(caller);
        }
    }

    function checkIfApiPayerOrOwner(address caller) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (
            !s.api_payers.contains(caller) &&
            caller != LibDiamond.contractOwner() &&
            caller != s.adminApiPayerAccount
        ) {
            revert AppStorage.OnlyApiPayerOrOwner(caller);
        }
    }

    /// @notice Non-reverting predicate: true if caller is an api payer, the
    ///         diamond owner, or the admin api payer account.
    function isApiPayerOrOwner(address caller) internal view returns (bool) {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        return
            s.api_payers.contains(caller) ||
            caller == LibDiamond.contractOwner() ||
            caller == s.adminApiPayerAccount;
    }

    /// @notice Resolve any API key hash (master or usage) to the master account hash.
    function resolveToMaster(uint256 apiKeyHash) internal view returns (uint256) {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        return s.allApiKeyHashesToMaster[apiKeyHash];
    }

    function canAccountAddPkpToGroup(
        uint256 usageApiKeyHash,
        uint256 groupId
    ) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        uint256 accountApiKeyHash = s.allApiKeyHashesToMaster[usageApiKeyHash];
        AppStorage.UsageApiKey storage usageApiKey = s
            .accounts[accountApiKeyHash]
            .usageApiKeys[usageApiKeyHash];
        if (!usageApiKey.addPkpToGroups.contains(groupId)) {
            revert AppStorage.NotAllowedToAddPkpToGroup(
                usageApiKeyHash,
                groupId
            );
        }
    }

    function canAccountRemovePkpFromGroup(
        uint256 usageApiKeyHash,
        uint256 groupId
    ) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        uint256 accountApiKeyHash = s.allApiKeyHashesToMaster[usageApiKeyHash];
        AppStorage.UsageApiKey storage usageApiKey = s
            .accounts[accountApiKeyHash]
            .usageApiKeys[usageApiKeyHash];
        if (!usageApiKey.removePkpFromGroups.contains(groupId)) {
            revert AppStorage.NotAllowedToRemovePkpFromGroup(
                usageApiKeyHash,
                groupId
            );
        }
    }

    function canAccountManageIPFSIdsInGroup(
        uint256 usageApiKeyHash,
        uint256 groupId
    ) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        uint256 accountApiKeyHash = s.allApiKeyHashesToMaster[usageApiKeyHash];
        AppStorage.UsageApiKey storage usageApiKey = s
            .accounts[accountApiKeyHash]
            .usageApiKeys[usageApiKeyHash];
        if (!usageApiKey.manageIPFSIdsInGroups.contains(groupId)) {
            revert AppStorage.NotAllowedToManageIPFSIdsInGroup(
                usageApiKeyHash,
                groupId
            );
        }
    }

    function canCreateGroup(uint256 usageApiKeyHash) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        uint256 accountApiKeyHash = s.allApiKeyHashesToMaster[usageApiKeyHash];
        AppStorage.UsageApiKey storage usageApiKey = s
            .accounts[accountApiKeyHash]
            .usageApiKeys[usageApiKeyHash];
        if (!usageApiKey.createGroups) {
            revert AppStorage.NotAllowedToCreateGroup(usageApiKeyHash);
        }
    }

    function canDeleteGroup(uint256 usageApiKeyHash) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        uint256 accountApiKeyHash = s.allApiKeyHashesToMaster[usageApiKeyHash];
        AppStorage.UsageApiKey storage usageApiKey = s
            .accounts[accountApiKeyHash]
            .usageApiKeys[usageApiKeyHash];
        if (!usageApiKey.deleteGroups) {
            revert AppStorage.NotAllowedToDeleteGroup(usageApiKeyHash);
        }
    }

    function canCreatePkp(uint256 usageApiKeyHash) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        uint256 accountApiKeyHash = s.allApiKeyHashesToMaster[usageApiKeyHash];
        AppStorage.UsageApiKey storage usageApiKey = s
            .accounts[accountApiKeyHash]
            .usageApiKeys[usageApiKeyHash];
        if (!usageApiKey.createPKPs) {
            revert AppStorage.NotAllowedToCreatePkp(usageApiKeyHash);
        }
    }
}
