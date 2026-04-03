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

library AppStorage {
    using EnumerableSet for EnumerableSet.UintSet;
    using EnumerableSet for EnumerableSet.AddressSet;

    bytes32 constant ACCOUNT_CONFIG_STORAGE_POSITION =
        keccak256("com.litprotocol.accountconfig.storage");

    error AccountDoesNotExist(uint256 apiKeyHash);
    error NoAccountAccess(uint256 apiKeyHash, address sender);
    error AccountAlreadyExists(uint256 apiKeyHash);
    error GroupDoesNotExist(uint256 apiKeyHash, uint256 groupId);
    error PkpDoesNotExist(uint256 apiKeyHash, uint256 groupId, address pkpId);
    error ActionDoesNotExist(
        uint256 apiKeyHash,
        uint256 groupId,
        uint256 cidHash
    );
    error UsageApiKeyDoesNotExist(uint256 apiKeyHash, uint256 usageApiKeyHash);
    error InsufficientBalance(uint256 apiKeyHash, uint256 amount);
    error OnlyApiPayer(address caller);
    error OnlyApiPayerOrPricingOperator(address caller);
    error OnlyApiPayerOrOwner(address caller);
    error NotMasterAccount(uint256 apiKeyHash);
    error NotAllowedToCreateGroup(uint256 apiKeyHash);
    error NotAllowedToDeleteGroup(uint256 apiKeyHash);
    error NotAllowedToCreatePkp(uint256 apiKeyHash);
    error NotAllowedToAddPkpToGroup(uint256 apiKeyHash, uint256 groupId);
    error NotAllowedToRemovePkpFromGroup(uint256 apiKeyHash, uint256 groupId);
    error NotAllowedToManageIPFSIdsInGroup(uint256 apiKeyHash, uint256 groupId);
    error InvalidRequest(string message);
    error OnlyConfigOperatorOrOwner(address caller);
    

    struct PkpData {
        uint256 id; // keccak256 of the pkp id - this is used to prove existence of the struct.
        address pkpId; // address of the pkp - also the wallet address
        string name; // name of the pkp
        string description; // description of the pkp
    }

    /// @notice Metadata struct for account, group, action, and wallet.
    struct Metadata {
        uint256 id; // keccak256 of the metadata id - this is used to prove existence of the struct.
        string name; // name of the metadata
        string description; // description of the metadata
    }

    /// @notice UsageApiKey struct for usage API keys.
    struct UsageApiKey {
        Metadata metadata; // name and description of the usage api key
        uint256 apiKeyHash; // keccak256 of the base64 encoded api key
        uint256 expiration; // expiration date of the api key
        uint256 balance; // balance of the api key - currently using stripe to manage this.  will be 0.
        EnumerableSet.UintSet executeInGroups; // whether the api key can run actions  ( the value is the group id)
        bool createGroups; // whether the api key can create groups
        bool deleteGroups; // whether the api key can delete groups
        bool createPKPs; // whether the api key can create PKPs
        EnumerableSet.UintSet manageIPFSIdsInGroups; // whether the api key can manage IPFS IDs in groups ( the value is the group id)
        EnumerableSet.UintSet addPkpToGroups; // whether the api key can add PKPs to groups ( the value is the group id)
        EnumerableSet.UintSet removePkpFromGroups; // whether the api key can remove PKPs from groups ( the value is the group id)
    }

    /// @notice Group struct for groups.
    struct Group {
        Metadata metadata; // name and description of the group
        EnumerableSet.UintSet cidHash; // keccak256 of an action ipfs cid.  0x0 is the wildcard for all actions.
        EnumerableSet.AddressSet pkpId; // keccak256 of a Wallet address.  0x0 is the wildcard for all wallets.
    }

    /// @notice Account struct for accounts.
    struct Account {
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
        mapping(uint256 => address) pkpIds; // mapping from an index ( pkpCount) to a pkp id allowing us to get a list of all pkp ids for this account
        mapping(address => Metadata) pkpData; // mapping from a wallet address to it's metadata.  the ID is the derivationPath.
        bool managed; // whether the LIT-node can help manage the key.
        uint256 pkpCount; // counter for creating unique pkp ids
        uint256 actionCount; // count of unique actions registered to this account
        uint256 groupCount; // counter for creating unique group ids
    }

    /// @notice Root storage for AccountConfig. Stored at a fixed slot so all facets share the same state.
    struct AccountConfigStorage {
        mapping(uint256 => Account) accounts; // mapping from a given api key to it's config
        mapping(uint256 => uint256) indexToAccountHash; // mapping from an index to an account hash
        mapping(uint256 => uint256) allApiKeyHashesToMaster; // mapping from any api key has to it's master account api key hash
        mapping(uint256 => address) allPkpIds; // mapping from a counter to a pkp id, allowing us to get a list of all pkp ids ever generated
        mapping(uint256 => uint256) pricing; // mapping from a pricing item id to it's price
        EnumerableSet.AddressSet api_payers; // set of accounts that pays for state mutation made by api calls, optionally mutates state on behalf of an api key holder.
        address configOperator; // account that can make operational changes.
        address pricingOperator; // account that can mutate certain state for operational purposes ( like pricing ).
        uint256 pkpCount; // counter for creating unique pkp ids
        uint256 accountCount; // counter for creating unique account ids
        address adminApiPayerAccount; // address of the default api payer
        uint256 requestedApiPayerCount; // number of accounts that are allowed to pay for state mutation made by api calls
        uint256 rebalanceAmount; // amount of ether to rebalance the api payers with.  If 0, then don't rebalance.
        EnumerableSet.StringSet nodeConfigurationKeys;
        mapping(string => string) nodeConfigurationValues;
        uint256 serverTriggerValue;
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

    /// @notice Returns whether the account exists and the caller is allowed to mutate it (api_payer for managed accounts, or the creator).
    /// @param apiKeyHash Keccak256 hash of the account or usage API key (resolves to master account).
    /// @return True if the account exists and msg.sender may mutate it.
    function accountExistsAndIsMutable(
        uint256 apiKeyHash,
        address sender
    ) internal view returns (bool) {
        AccountConfigStorage storage s = getStorage();
        uint256 masterAccountApiKeyHash = s.allApiKeyHashesToMaster[apiKeyHash];
        if (masterAccountApiKeyHash == 0) {
            return false;
        }
        Account storage account = s.accounts[masterAccountApiKeyHash];
        if (account.accountApiKey.apiKeyHash != masterAccountApiKeyHash)
            return false;
        if (s.api_payers.contains(sender) && account.managed == true)
            return true;
        return account.creatorWalletAddress == sender;
    }

    /// @notice Returns whether a group exists for the account (and reverts if account is not mutable by caller).
    /// @param accountApiKeyHash Keccak256 hash of the account or usage API key (resolves to master account).
    /// @param groupId The keccak256 of the group id.
    /// @return True if the group exists.
    function groupExists(
        uint256 accountApiKeyHash,
        uint256 groupId
    ) internal view returns (bool) {
        AccountConfigStorage storage s = getStorage();
        Account storage account = s.accounts[accountApiKeyHash];
        Group storage group = account.groups[groupId];
        return group.metadata.id == groupId;
    }

    function revertIfNoAccountAccess(
        uint256 accountApiKeyHash,
        address sender
    ) internal view {
        if (!accountExistsAndIsMutable(accountApiKeyHash, sender)) {
            revert NoAccountAccess(accountApiKeyHash, sender);
        }
    }

    function revertIfGroupDoesNotExist(
        uint256 accountApiKeyHash,
        uint256 groupId
    ) internal view {
        if (!groupExists(accountApiKeyHash, groupId)) {
            revert GroupDoesNotExist(accountApiKeyHash, groupId);
        }
    }

    function checkIfApiPayerOrPricingOperator(
        AccountConfigStorage storage s,
        address caller
    ) internal view {
        if (!s.api_payers.contains(caller) && caller != s.pricingOperator) {
            revert OnlyApiPayerOrPricingOperator(caller);
        }
    }
}
