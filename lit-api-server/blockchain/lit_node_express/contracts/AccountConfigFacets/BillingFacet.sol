/// @title AccountBilling
/// @author Brendon Paul
/// @notice Billing related functions for AccountConfig diamond.
/// @notice Stripe is currently used to manage billing.

// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {AppStorage} from "./AppStorage.sol";
import {SecurityLib} from "./SecurityLib.sol";

contract BillingFacet {
    using EnumerableSet for EnumerableSet.UintSet;
    using EnumerableSet for EnumerableSet.AddressSet;

    event ApiKeyDebited(uint256 indexed apiKeyHash, uint256 amount);
    event ApiKeyCredited(uint256 indexed apiKeyHash, uint256 amount);
    event PricingUpdated(uint256 indexed pricingItemId, uint256 price);
    event PricingOperatorUpdated(address indexed newPricingOperator);

    function getPricing(uint256 pricingItemId) public view returns (uint256) {
        return AppStorage.getStorage().pricing[pricingItemId];
    }

    function debitApiKey(uint256 apiKeyHash, uint256 amount) public {
        SecurityLib.revertIfNotApiPayerOrPricingOperator(msg.sender);
        SecurityLib.revertIfNoAccountAccess(apiKeyHash, msg.sender);
        SecurityLib.revertIfNotMasterAccount(apiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[apiKeyHash];
        if (account.accountApiKey.balance < amount) {
            revert AppStorage.InsufficientBalance(apiKeyHash, amount);
        }
        account.accountApiKey.balance -= amount;
        emit ApiKeyDebited(apiKeyHash, amount);
    }

    function creditApiKey(uint256 apiKeyHash, uint256 amount) public {
        SecurityLib.revertIfNotApiPayerOrPricingOperator(msg.sender);
        SecurityLib.revertIfNoAccountAccess(apiKeyHash, msg.sender);
        SecurityLib.revertIfNotMasterAccount(apiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[apiKeyHash];
        account.accountApiKey.balance += amount;
        emit ApiKeyCredited(apiKeyHash, amount);
    }

    function setPricing(uint256 pricingItemId, uint256 price) public {
        SecurityLib.revertIfNotApiPayerOrPricingOperator(msg.sender);
        AppStorage.getStorage().pricing[pricingItemId] = price;
        emit PricingUpdated(pricingItemId, price);
    }

    function setPricingOperator(address newPricingOperator) public {
        SecurityLib.revertIfNotOwner(msg.sender);
        AppStorage.getStorage().pricingOperator = newPricingOperator;
        emit PricingOperatorUpdated(newPricingOperator);
    }
}
