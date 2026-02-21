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
    error UsageApiKeyDoesNotExist(uint256 apiKey, uint256 usageApiKey);

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
        bool all_wallets_permitted; // whether all wallets are permitted to use the group
        bool all_actions_permitted; // whether all actions are permitted to use the group
    }

    struct Account {
        Metadata accountMetadata; // name and description of the account
        UsageApiKey accountApiKey; // the api key that is used to access the account
        address creatorWalletAddress; // wallet address of the creator of the account
        // Usage API Keys are rotatable keys that can be used to fund the account
        EnumerableSet.UintSet usageApiKeysList; // set of usage api keys that the account is a member of
        mapping(uint256 => UsageApiKey) usageApiKeys; // mapping from a keccak256 of a usage api key to it's config
        // Groups are collections of actions and optionally wallets that are associated with the account
        EnumerableSet.UintSet groupList; // list of all groups that the account has created
        mapping(uint256 => Group) groups; // mapping from an index of a group to it's config
        // Actions are the CID hashes of the actions that are permitted to be used in the group
        EnumerableSet.UintSet actionHashesList; // set of actions that the account is a member of
        mapping(uint256 => Metadata) actionMetadata; // mapping from a keccak256 of an action ipfs cid to it's metadata
        bool managed; // whether the LIT-node can help manage the key.
        mapping(uint256 => uint256) wallet_derivation; // mapping from a wallet address hash to it's derivation path
        // in theory the following two meta data mappings can be blank - can be helpful in a UI, though.
        mapping(uint256 => Metadata) walletDerivationMetadata; // mapping from a wallet address hash to it's metadata
        uint256 walletCount; // counter for creating unique wallet address hashes
        uint256 actionCount; // counter for creating unique action ids
        uint256 groupCount; // counter for creating unique group ids
    }

    // storage data for the account config
    mapping(uint256 => Account) accounts; // mapping from a given api key to it's config
    mapping(uint256 => uint256) walletAddressHashes; // mapping from a counter to a wallet address hash, allowing us to get a list of all wallet hashes ever generated
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
        address creatorWalletAddress,
        uint256 initialBalance
    ) public {
        Account storage account = accounts[apiKeyHash];
        account.managed = managed;
        account.creatorWalletAddress = creatorWalletAddress;
        account.accountMetadata.id = apiKeyHash;
        account.accountMetadata.name = accountName;
        account.accountMetadata.description = accountDescription;
        account.accountApiKey.apiKeyHash = apiKeyHash;
        account.accountApiKey.expiration = block.timestamp + 365 days * 10;
        account.accountApiKey.balance = initialBalance;
    }

    // This is a double use function - it checks if the account exists
    // and if the caller is able to mutate the account, providing a modicum of security.
    function accountExistsAndIsMutable(
        uint256 apiKeyHash
    ) public view returns (bool) {
        Account storage account = accounts[apiKeyHash];
        if (account.accountMetadata.id != apiKeyHash) {
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
            .usageApiKeys[usageApiKeyHash];
        apiKeyStorage.apiKeyHash = usageApiKeyHash;
        apiKeyStorage.expiration = expiration;
        apiKeyStorage.balance = balance;
        accounts[accountApiKeyHash].usageApiKeysList.add(usageApiKeyHash);
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

        Account storage account = accounts[accountApiKeyHash];
        account.groupList.add(account.groupCount);
        Group storage group = account.groups[account.groupCount];
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

        Account storage account = accounts[accountApiKeyHash];
        account.groups[groupId].metadata.name = name;
        account.groups[groupId].metadata.description = description;
        account.groups[groupId].all_wallets_permitted = all_wallets_permitted;
        account.groups[groupId].all_actions_permitted = all_actions_permitted;
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
        Account storage account = accounts[accountApiKeyHash];
        account.actionMetadata[actionHash].name = name;
        account.actionMetadata[actionHash].description = description;
    }

    function removeActionFromGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 action
    ) public {
        revertIfActionDoesNotExist(accountApiKeyHash, groupId, action);

        Account storage account = accounts[accountApiKeyHash];
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

        Account storage account = accounts[accountApiKeyHash];
        account.groups[groupId].Wallets_hash.remove(walletAddressHash);
    }

    function updateUsageApiKeyMetadata(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash,
        string memory name,
        string memory description
    ) public {
        revertIfUsageApiKeyDoesNotExist(accountApiKeyHash, usageApiKeyHash);
        Account storage account = accounts[accountApiKeyHash];    
        account.usageApiKeys[usageApiKeyHash].metadata.name = name;
        account.usageApiKeys[usageApiKeyHash].metadata.description = description;
    }

    function removeUsageApiKey(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash
    ) public {
        revertIfUsageApiKeyDoesNotExist(accountApiKeyHash, usageApiKeyHash);

        Account storage account = accounts[accountApiKeyHash];
        account.usageApiKeysList.remove(usageApiKeyHash);
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
        account.walletCount++;
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
    function usageApiKeyExists(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash
    ) private view returns (bool) {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        return
            accounts[accountApiKeyHash]
                .usageApiKeys[usageApiKeyHash]
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
        if (!usageApiKeyExists(accountApiKeyHash, usageApiKeyHash)) {
            revert UsageApiKeyDoesNotExist(accountApiKeyHash, usageApiKeyHash);
        }
    }

    function listGroups(
        uint256 accountApiKeyHash,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (Metadata[] memory) {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        Account storage account = accounts[accountApiKeyHash];

        if (pageSize > account.groupList.length()) {
            pageSize = account.groupList.length();
            pageNumber = 0;
        }

        uint256 startIndex = pageNumber * pageSize;
        uint256 endIndex = startIndex + pageSize;

        if (endIndex > account.groupList.length()) {
            endIndex = account.groupList.length();
        }

        uint256 pageLength = endIndex - startIndex;
        Metadata[] memory pageMetadata = new Metadata[](pageLength);
        for (uint256 i = 0; i < pageLength; i++) {
            pageMetadata[i] = account
                .groups[account.groupList.at(startIndex + i)]
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
            pageSize = nextWalletCount;
            pageNumber = 0;
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
            pageSize = group.Wallets_hash.length();
            pageNumber = 0;
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
            pageSize = group.permitted_actions_cid_hash.length();
            pageNumber = 0;
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
