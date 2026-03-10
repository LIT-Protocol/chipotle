/// @title AccountConfigWrite
/// @author Brendon Paul
/// @notice Security functions for AccountConfig diamond.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {AppStorage} from "./AppStorage.sol";

library SecurityLib {
    using EnumerableSet for EnumerableSet.UintSet;
    using EnumerableSet for EnumerableSet.AddressSet;

    // ----- internal view helpers (revert helpers call view from Views facet conceptually; we duplicate for simplicity) -----
    function revertIfNotMasterAccount(uint256 accountApiKeyHash) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (s.allApiKeyHashes[accountApiKeyHash] != accountApiKeyHash) {
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

    function revertIfNotApiPayerOrOwner(address caller) internal view {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (
            !s.api_payers.contains(caller) &&
            caller != s.owner &&
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
            caller != s.owner &&
            caller != s.adminApiPayerAccount
        ) {
            revert AppStorage.OnlyApiPayerOrOwner(caller);
        }
    }
}
