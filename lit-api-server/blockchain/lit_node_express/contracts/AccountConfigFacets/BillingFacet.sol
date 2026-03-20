/// @title AccountBilling
/// @author Brendon Paul
/// @notice Billing related functions for AccountConfig diamond.
/// @notice Stripe is currently used to manage billing.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {AppStorage} from "./AppStorage.sol";
import {SecurityLib} from "./SecurityLib.sol";

contract BillingFacet {
    using EnumerableSet for EnumerableSet.UintSet;
    using EnumerableSet for EnumerableSet.AddressSet;

    function getPricing(uint256 pricingItemId) public view returns (uint256) {
        return AppStorage.getStorage().pricing[pricingItemId];
    }

    function debitApiKey(uint256 apiKeyHash, uint256 amount) public {
        SecurityLib.revertIfNotApiPayerOrPricingOperator(msg.sender);
        SecurityLib.revertIfNoAccountAccess(apiKeyHash, msg.sender);
        SecurityLib.revertIfNotMasterAccount(apiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        uint256 masterAccountApiKeyHash = s.allApiKeyHashesToMaster[apiKeyHash];
        AppStorage.Account storage account = s.accounts[
            masterAccountApiKeyHash
        ];
        AppStorage.UsageApiKey storage usageApiKey = (account
            .accountApiKey
            .apiKeyHash == apiKeyHash)
            ? account.accountApiKey
            : account.usageApiKeys[apiKeyHash];
        if (usageApiKey.balance < amount) {
            revert AppStorage.InsufficientBalance(apiKeyHash, amount);
        }
        usageApiKey.balance -= amount;
    }

    function creditApiKey(uint256 apiKeyHash, uint256 amount) public {
        SecurityLib.revertIfNotApiPayerOrPricingOperator(msg.sender);
        SecurityLib.revertIfNoAccountAccess(apiKeyHash, msg.sender);
        SecurityLib.revertIfNotMasterAccount(apiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        uint256 masterAccountApiKeyHash = s.allApiKeyHashesToMaster[apiKeyHash];
        AppStorage.Account storage account = s.accounts[
            masterAccountApiKeyHash
        ];
        AppStorage.UsageApiKey storage usageApiKey = (account
            .accountApiKey
            .apiKeyHash == apiKeyHash)
            ? account.accountApiKey
            : account.usageApiKeys[apiKeyHash];
        usageApiKey.balance += amount;
    }

    function setPricing(uint256 pricingItemId, uint256 price) public {
        SecurityLib.revertIfNotApiPayerOrPricingOperator(msg.sender);
        AppStorage.getStorage().pricing[pricingItemId] = price;
    }

    function setPricingOperator(address newPricingOperator) public {
        SecurityLib.revertIfNotApiPayerOrPricingOperator(msg.sender);
        AppStorage.getStorage().pricingOperator = newPricingOperator;
    }
}
