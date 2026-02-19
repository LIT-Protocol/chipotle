// Notes:
// Mappings using uin256 are always keccak256 whatever is being mapped.
// Each struct contains a keccak256 that matches the mapping to prove existence of the struct.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// import enumerableSet from openzeppelin
import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";

// Probably want this:
// import { ReentrancyGuard } from "@openzeppelin/contracts/security/ReentrancyGuard.sol";

contract AccountConfig {
    using EnumerableSet for EnumerableSet.AddressSet;
    using EnumerableSet for EnumerableSet.UintSet;

    error AccountDoesNotExist(uint256 apiKey);
    error GroupDoesNotExist(uint256 apiKey, uint256 groupId);
    error WalletDoesNotExist(uint256 apiKey, uint256 groupId, uint256 Wallet);
    error ActionDoesNotExist(uint256 apiKey, uint256 groupId, uint256 action);
    error AccountApiKeyDoesNotExist(uint256 apiKey, uint256 accountApiKey);
    error UsageApiKeyDoesNotExist(uint256 apiKey, uint256 usageApiKey);
    error PageSizeTooLarge(uint256 pageSize);

    struct Metadata {
        uint256 id;
        string name;
        string description;
    }

    struct UsageApiKey {
        Metadata metadata;
        uint256 apiKeyHash; // keccak256 of the base64 encoded api key
        uint256 expiration; // expiration date of the api key
        uint256 balance; // balance of the api key - funds that it holds for payment?
    }

    struct Group {
        Metadata metadata; // name and description of the group
        EnumerableSet.UintSet permitted_actions_cid_hash; // keccak256 of an action ipfs cid
        EnumerableSet.UintSet Wallets_hash; // keccak256 of a Wallet public key
    }

    struct Account {
        uint256 accountApiKeyHash; // keccak256 of the api key
        address creatorWalletAddress; // wallet address of the creator of the account
        string accountName; // name of the account
        string accountDescription; // description of the account
        mapping(uint256 => UsageApiKey) usageApiKeyHashes; // mapping from a keccak256 of an api key to it's config
        EnumerableSet.UintSet group_list; // set of groups that the account is a member of
        mapping(uint256 => Group) groups; // mapping from an index of a group to it's config
        bool managed; // whether the LIT-node can help manage the key.
        uint256 nextGroupId; // counter for creating unique group ids
        mapping(uint256 => uint256) wallet_derivation; // mapping from a wallet address hash to it's derivation path
        // in theory the following two meta data mappings can be blank - can be helpful in a UI, though.
        mapping(uint256 => Metadata) walletDerivationMetadata; // mapping from a wallet address hash to it's metadata
        mapping(uint256 => Metadata) actionMetadata; // mapping from a keccak256 of an action ipfs cid to it's metadata
    }

    // storage data for the account config
    mapping(uint256 => Account) accounts; // mapping from a given api key to it's config
    mapping(uint256 => uint256) walletAddressHashes; // mapping from a counter to a wallet address hash
    address public owner;
    uint256 public nextWalletCount; // counter for creating unique wallet address hashes

    // default owner is the deployer of the contract
    constructor() {
        owner = msg.sender;
        nextWalletCount = 1;
    }

    // functions for creating and managing accounts
    function newAccount(
        uint256 apiKeyHash,
        bool managed,
        string memory accountName,
        string memory accountDescription,
        address creatorWalletAddress
    ) public {
        Account storage account = accounts[apiKeyHash];
        account.accountApiKeyHash = apiKeyHash;
        account.managed = managed;
        account.creatorWalletAddress = creatorWalletAddress;
        account.accountName = accountName;
        account.accountDescription = accountDescription;
    }

    // This is a double use function - it checks if the account exists
    // and if the caller is able to mutate the account, providing a modicum of security.
    function accountExistsAndIsMutable(
        uint256 apiKeyHash
    ) private view returns (bool) {
        Account storage account = accounts[apiKeyHash];
        if (account.accountApiKeyHash != apiKeyHash) {
            return false;
        }
        if (msg.sender == owner && account.managed == true) {
            return true;
        }

        // Accounts must be created by the owner wallet address.
        return account.creatorWalletAddress == msg.sender;
    }

    function addApiKey(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash,
        uint256 expiration,
        uint256 balance
    ) public {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        UsageApiKey storage apiKeyStorage = accounts[accountApiKeyHash]
            .usageApiKeyHashes[usageApiKeyHash];
        apiKeyStorage.apiKeyHash = usageApiKeyHash;
        apiKeyStorage.expiration = expiration;
        apiKeyStorage.balance = balance;
    }

    function addGroup(
        uint256 accountApiKeyHash,
        string memory name,
        string memory description,
        uint256[] memory permitted_actions,
        uint256[] memory Wallets
    ) public {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);

        Account storage account = accounts[accountApiKeyHash];
        account.group_list.add(account.nextGroupId);
        Group storage group = account.groups[account.nextGroupId];
        group.metadata.id = account.nextGroupId;
        group.metadata.name = name;
        group.metadata.description = description;
        for (uint256 i = 0; i < permitted_actions.length; i++) {
            group.permitted_actions_cid_hash.add(permitted_actions[i]);
        }
        for (uint256 i = 0; i < Wallets.length; i++) {
            group.Wallets_hash.add(Wallets[i]);
        }
        account.nextGroupId++;
    }

    function addWalletToGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 walletAddressHash
    ) public {
        revertIfGroupDoesNotExist(accountApiKeyHash, groupId);

        Account storage account = accounts[accountApiKeyHash];
        account.groups[groupId].Wallets_hash.add(walletAddressHash);
    }

    function addActionToGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 action,
        string memory name,
        string memory description
    ) public {
        revertIfGroupDoesNotExist(accountApiKeyHash, groupId);

        Account storage account = accounts[accountApiKeyHash];
        account.groups[groupId].permitted_actions_cid_hash.add(action);
        account.actionMetadata[action].id = action;
        account.actionMetadata[action].name = name;
        account.actionMetadata[action].description = description;
    }

    function removeActionFromGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 action
    ) public {
        revertIfActionDoesNotExist(accountApiKeyHash, groupId, action);

        Account storage account = accounts[accountApiKeyHash];
        account.groups[groupId].permitted_actions_cid_hash.remove(action);
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

        Account storage account = accounts[accountApiKeyHash];
        account.groups[groupId].Wallets_hash.remove(walletAddressHash);
    }

    function removeUsageApiKey(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash
    ) public {
        revertIfUsageApiKeyDoesNotExist(accountApiKeyHash, usageApiKeyHash);

        Account storage account = accounts[accountApiKeyHash];
        delete account.usageApiKeyHashes[usageApiKeyHash];
    }

    function registerWalletDerivation(
        uint256 accountApiKeyHash,
        uint256 walletAddressHash,
        uint256 derivationPath,
        string memory name,
        string memory description
    ) public {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        Account storage account = accounts[accountApiKeyHash];
        account.wallet_derivation[walletAddressHash] = derivationPath;
        account
            .walletDerivationMetadata[walletAddressHash]
            .id = walletAddressHash;
        account.walletDerivationMetadata[walletAddressHash].name = name;
        account
            .walletDerivationMetadata[walletAddressHash]
            .description = description;
        walletAddressHashes[nextWalletCount] = walletAddressHash;
        nextWalletCount++;
    }

    function getWalletDerivation(
        uint256 accountApiKeyHash,
        uint256 walletAddressHash
    ) public view returns (uint256) {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        Account storage account = accounts[accountApiKeyHash];
        return account.wallet_derivation[walletAddressHash];
    }

    // Existence checks
    function accountApiKeyExists(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash
    ) private view returns (bool) {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        return
            accounts[accountApiKeyHash]
                .usageApiKeyHashes[usageApiKeyHash]
                .apiKeyHash == usageApiKeyHash;
    }

    function groupExists(
        uint256 accountApiKeyHash,
        uint256 groupId
    ) private view returns (bool) {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        Group storage group = accounts[accountApiKeyHash].groups[groupId];
        return group.metadata.id == groupId;
    }

    function actionExists(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 action
    ) private view returns (bool) {
        revertIfGroupDoesNotExist(accountApiKeyHash, groupId);
        return
            accounts[accountApiKeyHash]
                .groups[groupId]
                .permitted_actions_cid_hash
                .contains(action);
    }

    function WalletExists(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 Wallet
    ) private view returns (bool) {
        revertIfGroupDoesNotExist(accountApiKeyHash, groupId);
        return
            accounts[accountApiKeyHash].groups[groupId].Wallets_hash.contains(
                Wallet
            );
    }

    // Revert checks - less expensive than decorators
    function revertIfAccountDoesNotExistAndIsMutable(
        uint256 accountApiKeyHash
    ) private view {
        if (!accountExistsAndIsMutable(accountApiKeyHash)) {
            revert AccountDoesNotExist(accountApiKeyHash);
        }
    }

    function revertIfGroupDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId
    ) private view {
        if (!groupExists(accountApiKeyHash, groupId)) {
            revert GroupDoesNotExist(accountApiKeyHash, groupId);
        }
    }

    function revertIfActionDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 action
    ) private view {
        if (!actionExists(accountApiKeyHash, groupId, action)) {
            revert ActionDoesNotExist(accountApiKeyHash, groupId, action);
        }
    }

    function revertIfWalletDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 Wallet
    ) private view {
        if (!WalletExists(accountApiKeyHash, groupId, Wallet)) {
            revert WalletDoesNotExist(accountApiKeyHash, groupId, Wallet);
        }
    }

    function revertIfUsageApiKeyDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash
    ) private view {
        if (!accountApiKeyExists(accountApiKeyHash, usageApiKeyHash)) {
            revert AccountApiKeyDoesNotExist(
                accountApiKeyHash,
                usageApiKeyHash
            );
        }
    }

    function listGroups(
        uint256 accountApiKeyHash,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (Metadata[] memory) {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        Account storage account = accounts[accountApiKeyHash];

        if (pageSize > account.group_list.length()) {
            revert PageSizeTooLarge(account.group_list.length());
        }

        uint256 startIndex = pageNumber * pageSize;
        uint256 endIndex = startIndex + pageSize;

        if (endIndex > account.group_list.length()) {
            endIndex = account.group_list.length();
        }

        uint256 pageLength = endIndex - startIndex;
        Metadata[] memory pageMetadata = new Metadata[](pageLength);
        for (uint256 i = 0; i < pageLength; i++) {
            pageMetadata[i] = account
                .groups[account.group_list.at(startIndex + i)]
                .metadata;
        }
        return pageMetadata;
    }

    function listWallets(
        uint256 accountApiKeyHash,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (Metadata[] memory) {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        Account storage account = accounts[accountApiKeyHash];

        if (pageSize > nextWalletCount) {
            revert PageSizeTooLarge(nextWalletCount);
        }

        uint256 startIndex = pageNumber * pageSize;
        uint256 endIndex = startIndex + pageSize;

        if (endIndex > nextWalletCount) {
            endIndex = nextWalletCount;
        }

        uint256 pageLength = endIndex - startIndex;
        Metadata[] memory pageMetadata = new Metadata[](pageLength);
        for (uint256 i = 0; i < pageLength; i++) {
            pageMetadata[i] = account.walletDerivationMetadata[
                walletAddressHashes[startIndex + i]
            ];
        }
        return pageMetadata;
    }

    function listWalletsInGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (Metadata[] memory) {
        revertIfGroupDoesNotExist(accountApiKeyHash, groupId);

        Account storage account = accounts[accountApiKeyHash];
        Group storage group = account.groups[groupId];

        if (pageSize > group.Wallets_hash.length()) {
            revert PageSizeTooLarge(group.Wallets_hash.length());
        }

        uint256 startIndex = pageNumber * pageSize;
        uint256 endIndex = startIndex + pageSize;

        if (endIndex > group.Wallets_hash.length()) {
            endIndex = group.Wallets_hash.length();
        }

        uint256 pageLength = endIndex - startIndex;
        Metadata[] memory pageMetadata = new Metadata[](pageLength);
        for (uint256 i = 0; i < pageLength; i++) {
            pageMetadata[i] = account.walletDerivationMetadata[
                group.Wallets_hash.at(startIndex + i)
            ];
        }
        return pageMetadata;
    }

    function listActions(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (Metadata[] memory) {
        revertIfGroupDoesNotExist(accountApiKeyHash, groupId);
        Account storage account = accounts[accountApiKeyHash];
        Group storage group = account.groups[groupId];

        if (pageSize > group.permitted_actions_cid_hash.length()) {
            revert PageSizeTooLarge(group.permitted_actions_cid_hash.length());
        }

        uint256 startIndex = pageNumber * pageSize;
        uint256 endIndex = startIndex + pageSize;

        if (endIndex > group.permitted_actions_cid_hash.length()) {
            endIndex = group.permitted_actions_cid_hash.length();
        }

        uint256 pageLength = endIndex - startIndex;
        Metadata[] memory pageMetadata = new Metadata[](pageLength);
        for (uint256 i = 0; i < pageLength; i++) {
            pageMetadata[i] = account.actionMetadata[
                group.permitted_actions_cid_hash.at(startIndex + i)
            ];
        }
        return pageMetadata;
    }
}
