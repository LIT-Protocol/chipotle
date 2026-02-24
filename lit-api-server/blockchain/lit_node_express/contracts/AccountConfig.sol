/// @title AccountConfig
/// @author Brendon Paul
/// @notice This contract is used to manage the configuration of accounts and their associated data.
/// @notice  Mappings using uin256 are always keccak256 whatever is being mapped.
/// @notice Each struct contains a keccak256 that matches the mapping to prove existence of the struct.

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
    error InsufficientBalance(uint256 apiKey, uint256 amount);
    error OnlyApiPayerOrPricingOperator(address caller);

    /// @notice Metadata struct for account, group, action, and wallet.
    struct Metadata {
        uint256 id;  // keccak256 of the metadata id - this is used to prove existence of the struct.
        string name; // name of the metadata
        string description; // description of the metadata
    }

    /// @notice UsageApiKey struct for usage API keys.
    struct UsageApiKey {
        Metadata metadata; // name and description of the usage api key
        uint256 apiKeyHash; // keccak256 of the base64 encoded api key
        uint256 expiration; // expiration date of the api key
        uint256 balance; // balance of the api key - funds that it holds for payment?
    }

    /// @notice Group struct for groups.
    struct Group {
        Metadata metadata; // name and description of the group
        EnumerableSet.UintSet permitted_actions_cid_hash; // keccak256 of an action ipfs cid
        EnumerableSet.UintSet Wallets_hash; // keccak256 of a Wallet public key
        bool all_wallets_permitted; // whether all wallets are permitted to use the group
        bool all_actions_permitted; // whether all actions are permitted to use the group
    }

    /// @notice Account struct for accounts.
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
        mapping(uint256 => uint256) walletAddressHash; // mapping from a wallet address hash to an index, allowing us to get a list of all wallet hashes for this account
        mapping(uint256 => uint256) wallet_derivation; // mapping from a wallet address hash to it's derivation path
        // in theory the following two meta data mappings can be blank - can be helpful in a UI, though.
        mapping(uint256 => Metadata) walletDerivationMetadata; // mapping from a wallet address hash to it's metadata
        uint256 walletCount; // counter for creating unique wallet address hashes
        uint256 actionCount; // counter for creating unique action ids
        uint256 groupCount; // counter for creating unique group ids
    }

    // storage data for the account config
    mapping(uint256 => Account) accounts; // mapping from a given api key to it's config
    mapping(uint256 => uint256) allApiKeyHashes; // mapping from any api key has to it's master account api key hash
    mapping(uint256 => uint256) allWalletAddressHashes; // mapping from a counter to a wallet address hash, allowing us to get a list of all wallet hashes ever generated
    mapping(uint256 => uint256) pricing; // mapping from a pricing item id to it's price
    address public api_payer;  // account that pays for state mutation made by api calls, optionally mutates state on behalf of an api key holder.
    address public pricing_operator; // account that can mutate certain state for operational purposes ( like pricing ).
    uint256 public nextWalletCount; // counter for creating unique wallet address hashes

    /// @notice Initializes the contract with the deployer as api_payer and pricing_operator, and sets initial pricing.
    constructor() {
        api_payer = msg.sender;
        pricing_operator = msg.sender;
        nextWalletCount = 1;
        pricing[1] = 1;                 
    }

    /// @notice Creates a new account with the given API key hash, metadata, and optional managed flag.
    /// @param apiKeyHash Keccak256 hash of the base64-encoded account API key.
    /// @param managed If true, the LIT-node (api_payer) may mutate the account on behalf of the holder.
    /// @param accountName Display name for the account.
    /// @param accountDescription Description of the account.
    /// @param creatorWalletAddress Wallet address of the account creator (used for access control).
    /// @param initialBalance Initial balance assigned to the account API key.
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
        allApiKeyHashes[apiKeyHash] = apiKeyHash; // feels redudent, but helps with lookups.
    }

    /// @notice Returns whether the account exists and the caller is allowed to mutate it (api_payer for managed accounts, or the creator).
    /// @param apiKeyHash Keccak256 hash of the account or usage API key (resolves to master account).
    /// @return True if the account exists and msg.sender may mutate it.
    function accountExistsAndIsMutable(
        uint256 apiKeyHash
    ) public view returns (bool) {

        if (allApiKeyHashes[apiKeyHash] == 0) {
            return false;
        }

        uint256 masterAccountApiKeyHash = allApiKeyHashes[apiKeyHash];

        Account storage account = accounts[masterAccountApiKeyHash];
        if (account.accountMetadata.id != masterAccountApiKeyHash) {
            return false;
        }
        if (msg.sender == api_payer && account.managed == true) {
            return true;
        }

        // Accounts should be created by the owner wallet address.
        // This is artificially done when using the API, and we use the API Key wallet address as the account creator.
        return account.creatorWalletAddress == msg.sender;
    }

    /// @notice Adds a usage API key to the account with the given expiration and balance.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param usageApiKeyHash Keccak256 hash of the new usage API key.
    /// @param expiration Expiration timestamp for the usage key.
    /// @param balance Initial balance for the usage key.
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
        allApiKeyHashes[usageApiKeyHash] = accountApiKeyHash;
    }

    /// @notice Creates a new group for the account with optional permitted actions, wallets, and permission flags.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param name Group display name.
    /// @param description Group description.
    /// @param permitted_actions Array of keccak256 hashes of permitted action IPFS CIDs.
    /// @param Wallets Array of keccak256 hashes of permitted wallet public keys.
    /// @param all_wallets_permitted If true, any wallet may use this group.
    /// @param all_actions_permitted If true, any action may be run in this group.
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

    /// @notice Updates a group's metadata and permission flags (name, description, all_wallets_permitted, all_actions_permitted).
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param groupId ID of the group to update.
    /// @param name New group name.
    /// @param description New group description.
    /// @param all_wallets_permitted Whether all wallets are permitted.
    /// @param all_actions_permitted Whether all actions are permitted.
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

    /// @notice Adds a wallet (by its address hash) to a group's permitted set.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param groupId ID of the group.
    /// @param walletAddressHash Keccak256 hash of the wallet public key/address to permit.
    function addWalletToGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 walletAddressHash
    ) public {
        revertIfGroupDoesNotExist(accountApiKeyHash, groupId);

        Account storage account = accounts[accountApiKeyHash];
        account.groups[groupId].Wallets_hash.add(walletAddressHash);
    }

    /// @notice Adds an action (by its CID hash) to a group and stores its metadata.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param groupId ID of the group.
    /// @param action Keccak256 hash of the action IPFS CID.
    /// @param name Action display name.
    /// @param description Action description.
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

    /// @notice Updates the name and description metadata for an action in a group.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param actionHash Keccak256 hash of the action IPFS CID.
    /// @param groupId ID of the group containing the action.
    /// @param name New action name.
    /// @param description New action description.
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

    /// @notice Removes an action from a group's permitted actions set.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param groupId ID of the group.
    /// @param action Keccak256 hash of the action IPFS CID to remove.
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

    /// @notice Removes a wallet from a group's permitted wallets set.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param groupId ID of the group.
    /// @param walletAddressHash Keccak256 hash of the wallet to remove.
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

    /// @notice Updates the name and description metadata for a usage API key.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param usageApiKeyHash Keccak256 hash of the usage API key.
    /// @param name New name for the usage key.
    /// @param description New description for the usage key.
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

    /// @notice Removes a usage API key from the account (key can no longer be used; balance is effectively forfeit).
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param usageApiKeyHash Keccak256 hash of the usage API key to remove.
    function removeUsageApiKey(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash
    ) public {
        revertIfUsageApiKeyDoesNotExist(accountApiKeyHash, usageApiKeyHash);

        Account storage account = accounts[accountApiKeyHash];
        account.usageApiKeysList.remove(usageApiKeyHash);
    }

    /// @notice Registers a wallet derivation (address hash, derivation path, and metadata) for the account.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param walletAddressHash Keccak256 hash of the wallet address/public key.
    /// @param derivationPath Derivation path index for the wallet.
    /// @param name Wallet display name.
    /// @param description Wallet description.
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
        account.walletAddressHash[account.walletCount] = walletAddressHash;
        allWalletAddressHashes[nextWalletCount] = walletAddressHash;
        account.walletCount++;
        nextWalletCount++;
    }

    /// @notice Returns the derivation path registered for a wallet address hash under the account.
    /// @param apiKeyHash Keccak256 hash of the account or usage API key.
    /// @param walletAddressHash Keccak256 hash of the wallet address.
    /// @return The derivation path (index) for the wallet.
    function getWalletDerivation(
        uint256 apiKeyHash,
        uint256 walletAddressHash
    ) public view returns (uint256) {
        revertIfAccountDoesNotExistAndIsMutable(apiKeyHash);
        uint256 masterAccountApiKeyHash = allApiKeyHashes[apiKeyHash];
        Account storage account = accounts[masterAccountApiKeyHash];
        return account.wallet_derivation[walletAddressHash];
    }

    /// @notice Returns whether a usage API key exists for the account (and reverts if account is not mutable by caller).
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

    /// @notice Returns whether a group exists for the account (and reverts if account is not mutable by caller).
    function groupExists(
        uint256 accountApiKeyHash,
        uint256 groupId
    ) private view returns (bool) {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        Group storage group = accounts[accountApiKeyHash].groups[groupId];
        return group.metadata.id == groupId;
    }

    /// @notice Returns whether an action is permitted in the given group.
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

    /// @notice Returns whether a wallet is in the group's permitted set.
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

    /// @notice Reverts with AccountDoesNotExist if the account does not exist or the caller may not mutate it.
    function revertIfAccountDoesNotExistAndIsMutable(
        uint256 accountApiKeyHash
    ) private view {
        if (!accountExistsAndIsMutable(accountApiKeyHash)) {
            revert AccountDoesNotExist(accountApiKeyHash);
        }
    }

    /// @notice Reverts with GroupDoesNotExist if the group does not exist for the account.
    function revertIfGroupDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId
    ) private view {
        if (!groupExists(accountApiKeyHash, groupId)) {
            revert GroupDoesNotExist(accountApiKeyHash, groupId);
        }
    }

    /// @notice Reverts with ActionDoesNotExist if the action is not in the group.
    function revertIfActionDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 action
    ) private view {
        if (!actionExists(accountApiKeyHash, groupId, action)) {
            revert ActionDoesNotExist(accountApiKeyHash, groupId, action);
        }
    }

    /// @notice Reverts with WalletDoesNotExist if the wallet is not in the group.
    function revertIfWalletDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 Wallet
    ) private view {
        if (!WalletExists(accountApiKeyHash, groupId, Wallet)) {
            revert WalletDoesNotExist(accountApiKeyHash, groupId, Wallet);
        }
    }

    /// @notice Reverts with UsageApiKeyDoesNotExist if the usage API key is not on the account.
    function revertIfUsageApiKeyDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash
    ) private view {
        if (!usageApiKeyExists(accountApiKeyHash, usageApiKeyHash)) {
            revert UsageApiKeyDoesNotExist(accountApiKeyHash, usageApiKeyHash);
        }
    }

    /// @notice Returns a paginated list of usage API keys (UsageApiKey structs) for the account.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param pageNumber Zero-based page index.
    /// @param pageSize Number of items per page.
    /// @return Array of UsageApiKey for the requested page.
    function listApiKeys(
        uint256 accountApiKeyHash,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (UsageApiKey[] memory) {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        Account storage account = accounts[accountApiKeyHash];
        uint256 pageLength = account.usageApiKeysList.length();
        if (pageSize > pageLength) {
            pageSize = pageLength;
            pageNumber = 0;
        }
        uint256 startIndex = pageNumber * pageSize;
        uint256 endIndex = startIndex + pageSize;
        if (endIndex > pageLength) {
            endIndex = pageLength;
        }
        UsageApiKey[] memory pageApiKeys = new UsageApiKey[](endIndex - startIndex);
        for (uint256 i = 0; i < pageLength; i++) {
            pageApiKeys[i] = account.usageApiKeys[account.usageApiKeysList.at(startIndex + i)];
        }
        return pageApiKeys;
    }

    /// @notice Returns a paginated list of group metadata for the account.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param pageNumber Zero-based page index.
    /// @param pageSize Number of items per page.
    /// @return Array of Metadata (id, name, description) for each group on the page.
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

    /// @notice Returns a paginated list of wallet derivation metadata for the account.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param pageNumber Zero-based page index.
    /// @param pageSize Number of items per page.
    /// @return Array of Metadata (id, name, description) for each wallet on the page.
    function listWallets(
        uint256 accountApiKeyHash,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (Metadata[] memory) {
        revertIfAccountDoesNotExistAndIsMutable(accountApiKeyHash);
        Account storage account = accounts[accountApiKeyHash];

        if (pageSize > account.walletCount) {
            pageSize = account.walletCount;
            pageNumber = 0;
        }

        uint256 startIndex = pageNumber * pageSize;
        uint256 endIndex = startIndex + pageSize;

        if (endIndex > account.walletCount) {
            endIndex = account.walletCount;
        }

        uint256 pageLength = endIndex - startIndex;
        Metadata[] memory pageMetadata = new Metadata[](pageLength);
        for (uint256 i = 0; i < pageLength; i++) {
            pageMetadata[i] = account.walletDerivationMetadata[
                account.walletAddressHash[startIndex + i]
            ];
        }
        return pageMetadata;
    }

    /// @notice Returns a paginated list of wallet metadata permitted in the given group.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param groupId ID of the group.
    /// @param pageNumber Zero-based page index.
    /// @param pageSize Number of items per page.
    /// @return Array of Metadata for wallets in the group on the requested page.
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

    /// @notice Returns a paginated list of action metadata permitted in the given group.
    /// @param accountApiKeyHash Keccak256 hash of the account API key.
    /// @param groupId ID of the group.
    /// @param pageNumber Zero-based page index.
    /// @param pageSize Number of items per page.
    /// @return Array of Metadata (id, name, description) for actions in the group on the requested page.
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

    /// @notice Decreases the balance of an account or usage API key by amount. Callable only by api_payer or pricing_operator.
    /// @param apiKeyHash Keccak256 hash of the account or usage API key to debit.
    /// @param amount Amount to subtract from the key's balance.
    function debitApiKey(
        uint256 apiKeyHash, 
        uint256 amount
    ) public {
        CheckIfApiPayerOrPricingOperator(msg.sender);
        revertIfAccountDoesNotExistAndIsMutable(apiKeyHash);
        uint256 masterAccountApiKeyHash = allApiKeyHashes[apiKeyHash];
        Account storage account = accounts[masterAccountApiKeyHash];
        UsageApiKey storage usageApiKey = (account.accountApiKey.apiKeyHash == apiKeyHash) ? account.accountApiKey : account.usageApiKeys[apiKeyHash];
        if (usageApiKey.balance < amount) {
            revert InsufficientBalance(apiKeyHash, amount);
        }
        usageApiKey.balance -= amount;
    }

    /// @notice Increases the balance of an account or usage API key by amount. Callable only by api_payer or pricing_operator.
    /// @param apiKeyHash Keccak256 hash of the account or usage API key to credit.
    /// @param amount Amount to add to the key's balance.
    function creditApiKey(
        uint256 apiKeyHash, 
        uint256 amount
    ) public {
        CheckIfApiPayerOrPricingOperator(msg.sender);
        revertIfAccountDoesNotExistAndIsMutable(apiKeyHash);
        uint256 masterAccountApiKeyHash = allApiKeyHashes[apiKeyHash];
        Account storage account = accounts[masterAccountApiKeyHash];
        UsageApiKey storage usageApiKey = (account.accountApiKey.apiKeyHash == apiKeyHash) ? account.accountApiKey : account.usageApiKeys[apiKeyHash];
        usageApiKey.balance += amount;
    }

    /// @notice Returns the price for a pricing item (e.g. cost per Lit Action run).
    /// @param pricingItemId ID of the pricing item.
    /// @return The price (e.g. in wei or smallest unit).
    function getPricing(
        uint256 pricingItemId
    ) public view returns (uint256) {
        return pricing[pricingItemId];
    }

    /// @notice Sets the price for a pricing item. Callable only by api_payer or pricing_operator.
    /// @param pricingItemId ID of the pricing item.
    /// @param price New price value.
    function setPricing(
        uint256 pricingItemId,
        uint256 price
    ) public {
        CheckIfApiPayerOrPricingOperator(msg.sender);
        pricing[pricingItemId] = price;
    }

    /// @notice Reverts with OnlyApiPayerOrPricingOperator if caller is neither api_payer nor pricing_operator.
    function CheckIfApiPayerOrPricingOperator(
        address caller
    ) internal view {
        if (caller != api_payer && caller != pricing_operator) {
            revert OnlyApiPayerOrPricingOperator(caller);    
        }        
    }

    /// @notice Sets the pricing_operator address. Callable only by the current api_payer or pricing_operator.
    /// @param newPricingOperator Address of the new pricing operator.
    function setPricingOperator(
        address newPricingOperator
    ) public {
        CheckIfApiPayerOrPricingOperator(msg.sender);
        pricing_operator = newPricingOperator;
    }
}
