/// @title AccountConfigFacet
/// @author Brendon Paul
/// @notice Mutable (state-changing) functions for AccountConfig diamond.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {LibAccountConfigStorage} from "./LibAccountConfigStorage.sol";

contract AccountConfigFacet {
    using EnumerableSet for EnumerableSet.UintSet;

    /// @notice Initializes storage. Must be called once after deployment (e.g. by Diamond constructor).
    function initializeAccountConfig() internal {
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        require(s.api_payer == address(0), "already initialized");
        s.api_payer = msg.sender;
        s.pricing_operator = msg.sender;
        s.nextWalletCount = 1;
        s.pricing[1] = 1;
    }

    function newAccount(
        uint256 apiKeyHash,
        bool managed,
        string memory accountName,
        string memory accountDescription,
        address creatorWalletAddress,
        uint256 initialBalance
    ) public {
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Account storage account = s.accounts[
            apiKeyHash
        ];
        account.managed = managed;
        account.creatorWalletAddress = creatorWalletAddress;
        account.accountMetadata.id = apiKeyHash;
        account.accountMetadata.name = accountName;
        account.accountMetadata.description = accountDescription;
        account.accountApiKey.apiKeyHash = apiKeyHash;
        account.accountApiKey.expiration = block.timestamp + 365 days * 10;
        account.accountApiKey.balance = initialBalance;
        s.allApiKeyHashes[apiKeyHash] = apiKeyHash;
    }

    function addApiKey(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash,
        uint256 expiration,
        uint256 balance
    ) public {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        uint256 masterAccountApiKeyHash = s.allApiKeyHashes[
            accountApiKeyHash
        ];
        LibAccountConfigStorage.UsageApiKey storage apiKeyStorage = s
            .accounts[masterAccountApiKeyHash]
            .usageApiKeys[usageApiKeyHash];
        apiKeyStorage.apiKeyHash = usageApiKeyHash;
        apiKeyStorage.expiration = expiration;
        apiKeyStorage.balance = balance;
        s
            .accounts[masterAccountApiKeyHash]
            .usageApiKeysList
            .add(usageApiKeyHash);
        s.allApiKeyHashes[usageApiKeyHash] = masterAccountApiKeyHash;
    }

    function addGroup(
        uint256 accountApiKeyHash,
        string memory name,
        string memory description,
        uint256[] memory permitted_actions,
        uint256[] memory Wallets,
        bool all_wallets_permitted,
        bool all_actions_permitted
    ) public {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Account storage account = s.accounts[
            accountApiKeyHash
        ];
        account.groupList.add(account.groupCount);
        LibAccountConfigStorage.Group storage group = account.groups[
            account.groupCount
        ];
        group.metadata.id = account.groupCount;
        group.metadata.name = name;
        group.metadata.description = description;
        for (uint256 i = 0; i < permitted_actions.length; i++) {
            group.permitted_actions_cid_hash.add(permitted_actions[i]);
        }
        for (uint256 i = 0; i < Wallets.length; i++) {
            group.Wallets_hash.add(Wallets[i]);
        }
        group.all_wallets_permitted = all_wallets_permitted;
        group.all_actions_permitted = all_actions_permitted;
        account.groupCount++;
    }

    function updateGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        string memory name,
        string memory description,
        bool all_wallets_permitted,
        bool all_actions_permitted
    ) public {
        revertIfGroupDoesNotExist(accountApiKeyHash, groupId);
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Group storage group = s
            .accounts[accountApiKeyHash]
            .groups[groupId];
        group.metadata.name = name;
        group.metadata.description = description;
        group.all_wallets_permitted = all_wallets_permitted;
        group.all_actions_permitted = all_actions_permitted;
    }

    function addWalletToGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 walletAddressHash
    ) public {
        revertIfGroupDoesNotExist(accountApiKeyHash, groupId);
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        s.accounts[accountApiKeyHash].groups[groupId].Wallets_hash.add(
            walletAddressHash
        );
    }

    function addActionToGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 action,
        string memory name,
        string memory description
    ) public {
        revertIfGroupDoesNotExist(accountApiKeyHash, groupId);
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Account storage account = s.accounts[
            accountApiKeyHash
        ];
        account.groups[groupId].permitted_actions_cid_hash.add(action);
        account.actionMetadata[action].id = action;
        account.actionMetadata[action].name = name;
        account.actionMetadata[action].description = description;
        account.actionCount++;
    }

    function updateActionMetadata(
        uint256 accountApiKeyHash,
        uint256 actionHash,
        uint256 groupId,
        string memory name,
        string memory description
    ) public {
        revertIfActionDoesNotExist(accountApiKeyHash, groupId, actionHash);
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Account storage account = s.accounts[
            accountApiKeyHash
        ];
        account.actionMetadata[actionHash].name = name;
        account.actionMetadata[actionHash].description = description;
    }

    function removeActionFromGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 action
    ) public {
        revertIfActionDoesNotExist(accountApiKeyHash, groupId, action);
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Account storage account = s.accounts[
            accountApiKeyHash
        ];
        account.groups[groupId].permitted_actions_cid_hash.remove(action);
        if (account.actionCount > 0) {
            account.actionCount--;
        }
    }

    function removeWalletFromGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 walletAddressHash
    ) public {
        revertIfWalletDoesNotExist(
            accountApiKeyHash,
            groupId,
            walletAddressHash
        );
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        s.accounts[accountApiKeyHash].groups[groupId].Wallets_hash.remove(
            walletAddressHash
        );
    }

    function updateUsageApiKeyMetadata(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash,
        string memory name,
        string memory description
    ) public {
        revertIfUsageApiKeyDoesNotExist(accountApiKeyHash, usageApiKeyHash);
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        s
            .accounts[accountApiKeyHash]
            .usageApiKeys[usageApiKeyHash]
            .metadata
            .name = name;
        s
            .accounts[accountApiKeyHash]
            .usageApiKeys[usageApiKeyHash]
            .metadata
            .description = description;
    }

    function removeUsageApiKey(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash
    ) public {
        revertIfUsageApiKeyDoesNotExist(accountApiKeyHash, usageApiKeyHash);
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Account storage account = s.accounts[
            accountApiKeyHash
        ];
        account.usageApiKeysList.remove(usageApiKeyHash);
        delete account.usageApiKeys[usageApiKeyHash];
        delete s.allApiKeyHashes[usageApiKeyHash];
    }

    function registerWalletDerivation(
        uint256 accountApiKeyHash,
        uint256 walletAddressHash,
        uint256 derivationPath,
        string memory name,
        string memory description
    ) public {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Account storage account = s.accounts[
            accountApiKeyHash
        ];
        account.wallet_derivation[walletAddressHash] = derivationPath;
        account
            .walletDerivationMetadata[walletAddressHash]
            .id = walletAddressHash;
        account.walletDerivationMetadata[walletAddressHash].name = name;
        account
            .walletDerivationMetadata[walletAddressHash]
            .description = description;
        account.walletAddressHash[account.walletCount] = walletAddressHash;
        s.allWalletAddressHashes[s.nextWalletCount] = walletAddressHash;
        account.walletCount++;
        s.nextWalletCount++;
    }

    function debitApiKey(uint256 apiKeyHash, uint256 amount) public {
        checkIfApiPayerOrPricingOperator(msg.sender);
        revertIfAccountDoesNotExistAndIsMutable(apiKeyHash);
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        uint256 masterAccountApiKeyHash = s.allApiKeyHashes[apiKeyHash];
        LibAccountConfigStorage.Account storage account = s.accounts[
            masterAccountApiKeyHash
        ];
        LibAccountConfigStorage.UsageApiKey storage usageApiKey = (account
            .accountApiKey
            .apiKeyHash == apiKeyHash)
            ? account.accountApiKey
            : account.usageApiKeys[apiKeyHash];
        if (usageApiKey.balance < amount) {
            revert LibAccountConfigStorage.InsufficientBalance(
                apiKeyHash,
                amount
            );
        }
        usageApiKey.balance -= amount;
    }

    function creditApiKey(uint256 apiKeyHash, uint256 amount) public {
        checkIfApiPayerOrPricingOperator(msg.sender);
        revertIfAccountDoesNotExistAndIsMutable(apiKeyHash);
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        uint256 masterAccountApiKeyHash = s.allApiKeyHashes[apiKeyHash];
        LibAccountConfigStorage.Account storage account = s.accounts[
            masterAccountApiKeyHash
        ];
        LibAccountConfigStorage.UsageApiKey storage usageApiKey = (account
            .accountApiKey
            .apiKeyHash == apiKeyHash)
            ? account.accountApiKey
            : account.usageApiKeys[apiKeyHash];
        usageApiKey.balance += amount;
    }

    function setPricing(uint256 pricingItemId, uint256 price) public {
        checkIfApiPayerOrPricingOperator(msg.sender);
        LibAccountConfigStorage.getStorage().pricing[pricingItemId] = price;
    }

    function setPricingOperator(address newPricingOperator) public {
        checkIfApiPayerOrPricingOperator(msg.sender);
        LibAccountConfigStorage
            .getStorage()
            .pricing_operator = newPricingOperator;
    }

    // ----- internal view helpers (revert helpers call view from Views facet conceptually; we duplicate for simplicity) -----

    function revertIfAccountDoesNotExistAndIsMutable(
        uint256 accountApiKeyHash
    ) private view {
        LibAccountConfigStorage.revertIfAccountDoesNotExistAndIsMutable(
            LibAccountConfigStorage.getStorage(),
            accountApiKeyHash,
            msg.sender
        );
    }

    function revertIfGroupDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId
    ) private view {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        LibAccountConfigStorage.revertIfGroupDoesNotExist(
            LibAccountConfigStorage.getStorage(),
            accountApiKeyHash,
            groupId
        );
    }

    function revertIfActionDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 action
    ) private view {
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        if (
            !s
                .accounts[accountApiKeyHash]
                .groups[groupId]
                .permitted_actions_cid_hash
                .contains(action)
        ) {
            revert LibAccountConfigStorage.ActionDoesNotExist(
                accountApiKeyHash,
                groupId,
                action
            );
        }
    }

    function revertIfWalletDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 Wallet
    ) private view {
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        if (
            !s
                .accounts[accountApiKeyHash]
                .groups[groupId]
                .Wallets_hash
                .contains(Wallet)
        ) {
            revert LibAccountConfigStorage.WalletDoesNotExist(
                accountApiKeyHash,
                groupId,
                Wallet
            );
        }
    }

    function revertIfUsageApiKeyDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash
    ) private view {
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        if (
            s
                .accounts[accountApiKeyHash]
                .usageApiKeys[usageApiKeyHash]
                .apiKeyHash != usageApiKeyHash
        ) {
            revert LibAccountConfigStorage.UsageApiKeyDoesNotExist(
                accountApiKeyHash,
                usageApiKeyHash
            );
        }
    }

    function checkIfApiPayerOrPricingOperator(address caller) private view {
        LibAccountConfigStorage.checkIfApiPayerOrPricingOperator(
            LibAccountConfigStorage.getStorage(),
            caller
        );
    }
}
