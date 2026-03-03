/// @title LibAccountConfigStorage
/// @author Brendon Paul
/// @notice Storage layout and structs for AccountConfig diamond. All facets read/write this storage at a fixed slot.
/// @notice Mappings using uint256 are always keccak256 of whatever is being mapped.
/// @notice Each struct contains a keccak256 that matches the mapping to prove existence of the struct.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";

library LibAccountConfigStorage {
    using EnumerableSet for EnumerableSet.UintSet;

    bytes32 constant ACCOUNT_CONFIG_STORAGE_POSITION =
        keccak256("com.litprotocol.accountconfig.storage");

    error AccountDoesNotExist(uint256 apiKey);
    error GroupDoesNotExist(uint256 apiKey, uint256 groupId);
    error WalletDoesNotExist(uint256 apiKey, uint256 groupId, uint256 Wallet);
    error ActionDoesNotExist(uint256 apiKey, uint256 groupId, uint256 action);
    error UsageApiKeyDoesNotExist(uint256 apiKey, uint256 usageApiKey);
    error InsufficientBalance(uint256 apiKey, uint256 amount);
    error OnlyApiPayerOrPricingOperator(address caller);

    /// @notice Metadata struct for account, group, action, and wallet.
    struct Metadata {
        uint256 id;
        string name;
        string description;
    }

    /// @notice UsageApiKey struct for usage API keys.
    struct UsageApiKey {
        Metadata metadata;
        uint256 apiKeyHash;
        uint256 expiration;
        uint256 balance;
    }

    /// @notice Group struct for groups.
    struct Group {
        Metadata metadata;
        EnumerableSet.UintSet permitted_actions_cid_hash;
        EnumerableSet.UintSet Wallets_hash;
        bool all_wallets_permitted;
        bool all_actions_permitted;
    }

    /// @notice Account struct for accounts.
    struct Account {
        Metadata accountMetadata;
        UsageApiKey accountApiKey;
        address creatorWalletAddress;
        EnumerableSet.UintSet usageApiKeysList;
        mapping(uint256 => UsageApiKey) usageApiKeys;
        EnumerableSet.UintSet groupList;
        mapping(uint256 => Group) groups;
        EnumerableSet.UintSet actionHashesList;
        mapping(uint256 => Metadata) actionMetadata;
        bool managed;
        mapping(uint256 => uint256) walletAddressHash;
        mapping(uint256 => uint256) wallet_derivation;
        mapping(uint256 => Metadata) walletDerivationMetadata;
        uint256 walletCount;
        uint256 actionCount;
        uint256 groupCount;
    }

    /// @notice Root storage for AccountConfig. Stored at a fixed slot so all facets share the same state.
    struct AccountConfigStorage {
        mapping(uint256 => Account) accounts;
        mapping(uint256 => uint256) allApiKeyHashes;
        mapping(uint256 => uint256) allWalletAddressHashes;
        mapping(uint256 => uint256) pricing;
        address api_payer;
        address pricing_operator;
        uint256 nextWalletCount;
    }

    function getStorage()
        internal
        pure
        returns (AccountConfigStorage storage s)
    {
        bytes32 position = ACCOUNT_CONFIG_STORAGE_POSITION;
        assembly {
            s.slot := position
        }
    }

    function accountExistsAndIsMutable(
        AccountConfigStorage storage s,
        uint256 apiKeyHash,
        address sender
    ) internal view returns (bool) {
        if (s.allApiKeyHashes[apiKeyHash] == 0) return false;
        uint256 masterAccountApiKeyHash = s.allApiKeyHashes[apiKeyHash];
        Account storage account = s.accounts[masterAccountApiKeyHash];
        if (account.accountMetadata.id != masterAccountApiKeyHash) return false;
        if (sender == s.api_payer && account.managed == true) return true;
        return account.creatorWalletAddress == sender;
    }

    function groupExists(
        AccountConfigStorage storage s,
        uint256 accountApiKeyHash,
        uint256 groupId
    ) internal view returns (bool) {
        Account storage account = s.accounts[accountApiKeyHash];
        Group storage group = account.groups[groupId];
        return group.metadata.id == groupId;
    }

    function revertIfAccountDoesNotExistAndIsMutable(
        AccountConfigStorage storage s,
        uint256 accountApiKeyHash,
        address sender
    ) internal view {
        if (!accountExistsAndIsMutable(s, accountApiKeyHash, sender)) {
            revert AccountDoesNotExist(accountApiKeyHash);
        }
    }

    function revertIfGroupDoesNotExist(
        AccountConfigStorage storage s,
        uint256 accountApiKeyHash,
        uint256 groupId
    ) internal view {
        if (!groupExists(s, accountApiKeyHash, groupId)) {
            revert GroupDoesNotExist(accountApiKeyHash, groupId);
        }
    }

    function checkIfApiPayerOrPricingOperator(
        AccountConfigStorage storage s,
        address caller
    ) internal view {
        if (caller != s.api_payer && caller != s.pricing_operator) {
            revert OnlyApiPayerOrPricingOperator(caller);
        }
    }
}
