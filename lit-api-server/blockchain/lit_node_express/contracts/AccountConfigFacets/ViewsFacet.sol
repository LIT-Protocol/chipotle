/// @title AccountConfigViews
/// @author Brendon Paul
/// @notice View (read-only) functions for AccountConfig diamond.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {AppStorage} from "./AppStorage.sol";

contract ViewsFacet {
    using EnumerableSet for EnumerableSet.UintSet;
    using EnumerableSet for EnumerableSet.AddressSet;
    using EnumerableSet for EnumerableSet.StringSet;

    // used for returning data from the listApiKeys function.
    struct UsageApiKeyReturn {
        AppStorage.Metadata metadata; // name and description of the usage api key
        uint256 apiKeyHash; // keccak256 of the base64 encoded api key
        uint256 expiration; // expiration date of the api key
        uint256 balance; // balance of the api key - currently using stripe to manage this.  will be 0.
        uint256[] executeInGroups; // whether the api key can run actions
        bool createGroups; // whether the api key can create groups
        bool deleteGroups; // whether the api key can delete groups
        bool createPKPs; // whether the api key can create PKPs
        uint256[] manageIPFSIdsInGroups; // whether the api key can manage IPFS IDs in groups
        uint256[] addPkpToGroups; // whether the api key can add PKPs to groups
        uint256[] removePkpFromGroups; // whether the api key can remove PKPs from groups
    }

    struct GroupReturn {
        AppStorage.Metadata metadata; // name and description of the group
        uint256[] cidHash; // keccak256 of an action ipfs cid
        address[] pkpId; // wallet address
    }

    struct KeyValueReturn {
        string key;
        string value;
    }

    /// @notice Getters for public state (ABI compatibility with original AccountConfig).
    function adminApiPayerAccount() public view returns (address) {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        return s.adminApiPayerAccount;
    }

    function api_payers() public view returns (address[] memory) {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        return s.api_payers.values();
    }

    function pricingOperator() public view returns (address) {
        return AppStorage.getStorage().pricingOperator;
    }

    function configOperator() public view returns (address) {
        return AppStorage.getStorage().configOperator;
    }

    function pkpCount() public view returns (uint256) {
        return AppStorage.getStorage().pkpCount;
    }
    function accountCount() public view returns (uint256) {
        return AppStorage.getStorage().accountCount;
    }
    function indexToAccountHashAt(uint256 index) public view returns (uint256) {
        return AppStorage.getStorage().indexToAccountHash[index];
    }
    function allPkpIdsAt(uint256 index) public view returns (address) {
        return AppStorage.getStorage().allPkpIds[index];
    }
    function pricingAt(uint256 index) public view returns (uint256) {
        return AppStorage.getStorage().pricing[index];
    }

    function apiPayerCount() public view returns (uint256) {
        return AppStorage.getStorage().api_payers.length();
    }
    function requestedApiPayerCount() public view returns (uint256) {
        return AppStorage.getStorage().requestedApiPayerCount;
    }

    function rebalanceAmount() public view returns (uint256) {
        return AppStorage.getStorage().rebalanceAmount;
    }

    function nodeConfigurationKeys() public view returns (string[] memory) {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        return s.nodeConfigurationKeys.values();
    }
    function nodeConfigurationValue(string memory key) public view returns (string memory) {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        return s.nodeConfigurationValues[key];
    }

    function nodeConfigurationValues() public view returns (KeyValueReturn[] memory) {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        uint256 length = s.nodeConfigurationKeys.length();
        KeyValueReturn[] memory values = new KeyValueReturn[](length);
        for (uint256 i = 0; i < length; i++) {
            values[i] = KeyValueReturn(s.nodeConfigurationKeys.at(i), s.nodeConfigurationValues[s.nodeConfigurationKeys.at(i)]);
        }
        return values;
    }

    function accountExistsAndIsMutable(
        uint256 apiKeyHash
    ) public view returns (bool) {
        return AppStorage.accountExistsAndIsMutable(apiKeyHash, msg.sender);
    }

    /// @notice Return the creator wallet address for the account that owns the given API key.
    /// @dev Works with both master and usage API key hashes (resolves via allApiKeyHashesToMaster).
    /// @param apiKeyHash keccak256 of a master or usage API key (base64-encoded).
    /// @return The creator wallet address stored on the resolved master account.
    function getAccountWalletAddress(
        uint256 apiKeyHash
    ) public view returns (address) {
        AppStorage.Account storage account = getReadOnlyAccount(apiKeyHash);
        return account.creatorWalletAddress;
    }

    function getWalletDerivation(
        uint256 apiKeyHash,
        address walletAddress
    ) public view returns (uint256) {
        AppStorage.Account storage account = getReadOnlyAccount(apiKeyHash);
        return account.pkpData[walletAddress].id;
    }

    function listApiKeys(
        uint256 accountApiKeyHash,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (UsageApiKeyReturn[] memory) {
        AppStorage.Account storage account = getReadOnlyAccount(
            accountApiKeyHash
        );
        uint256 totalLength = account.usageApiKeysList.length();
        (uint256 startIndex, uint256 pageLength) = getPageStartAndLength(
            totalLength,
            pageNumber,
            pageSize
        );
        UsageApiKeyReturn[] memory pageApiKeys = new UsageApiKeyReturn[](
            pageLength
        );
        for (uint256 i = 0; i < pageLength; i++) {
            uint256 usageApiKeyHash = account.usageApiKeysList.at(
                startIndex + i
            );
            pageApiKeys[i] = UsageApiKeyReturn(
                account.usageApiKeys[usageApiKeyHash].metadata,
                account.usageApiKeys[usageApiKeyHash].apiKeyHash,
                account.usageApiKeys[usageApiKeyHash].expiration,
                account.usageApiKeys[usageApiKeyHash].balance,
                account.usageApiKeys[usageApiKeyHash].executeInGroups.values(),
                account.usageApiKeys[usageApiKeyHash].createGroups,
                account.usageApiKeys[usageApiKeyHash].deleteGroups,
                account.usageApiKeys[usageApiKeyHash].createPKPs,
                account
                    .usageApiKeys[usageApiKeyHash]
                    .manageIPFSIdsInGroups
                    .values(),
                account.usageApiKeys[usageApiKeyHash].addPkpToGroups.values(),
                account
                    .usageApiKeys[usageApiKeyHash]
                    .removePkpFromGroups
                    .values()
            );
        }
        return pageApiKeys;
    }

    function listGroups(
        uint256 accountApiKeyHash,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (AppStorage.Metadata[] memory) {
        AppStorage.Account storage account = getReadOnlyAccount(
            accountApiKeyHash
        );
        uint256 totalLength = account.groupList.length();
        (uint256 startIndex, uint256 pageLength) = getPageStartAndLength(
            totalLength,
            pageNumber,
            pageSize
        );
        AppStorage.Metadata[] memory pageMetadata = new AppStorage.Metadata[](
            pageLength
        );
        for (uint256 i = 0; i < pageLength; i++) {
            pageMetadata[i] = account
                .groups[account.groupList.at(startIndex + i)]
                .metadata;
        }
        return pageMetadata;
    }

    function listGroupContents(
        uint256 accountApiKeyHash,
        uint256 groupId
    ) public view returns (GroupReturn memory) {
        AppStorage.Account storage account = getReadOnlyAccount(
            accountApiKeyHash
        );

        GroupReturn memory groupReturn = GroupReturn(
            account.groups[groupId].metadata,
            account.groups[groupId].cidHash.values(),
            account.groups[groupId].pkpId.values()
        );
        return groupReturn;
    }

    function listPkps(
        uint256 accountApiKeyHash,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (AppStorage.PkpData[] memory) {
        AppStorage.Account storage account = getReadOnlyAccount(
            accountApiKeyHash
        );
        (uint256 startIndex, uint256 pageLength) = getPageStartAndLength(
            account.pkpCount,
            pageNumber,
            pageSize
        );

        AppStorage.PkpData[] memory pagePkpData = new AppStorage.PkpData[](
            pageLength
        );
        for (uint256 i = 0; i < pageLength; i++) {
            address walletAddress = account.pkpIds[startIndex + i];
            pagePkpData[i].pkpId = walletAddress;
            pagePkpData[i].name = account.pkpData[walletAddress].name;
            pagePkpData[i].description = account
                .pkpData[walletAddress]
                .description;
        }
        return pagePkpData;
    }

    function listWalletsInGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (AppStorage.PkpData[] memory) {
        AppStorage.Account storage account = getReadOnlyAccount(
            accountApiKeyHash
        );
        AppStorage.Group storage group = account.groups[groupId];
        (uint256 startIndex, uint256 pageLength) = getPageStartAndLength(
            group.pkpId.length(),
            pageNumber,
            pageSize
        );
        AppStorage.PkpData[] memory pagePkpData = new AppStorage.PkpData[](
            pageLength
        );
        for (uint256 i = 0; i < pageLength; i++) {
            address walletAddress = group.pkpId.at(startIndex + i);
            pagePkpData[i].pkpId = walletAddress;
            pagePkpData[i].name = account.pkpData[walletAddress].name;
            pagePkpData[i].description = account
                .pkpData[walletAddress]
                .description;
        }
        return pagePkpData;
    }

    function listActions(
        uint256 accountApiKeyHash,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (AppStorage.Metadata[] memory) {
        AppStorage.Account storage account = getReadOnlyAccount(
            accountApiKeyHash
        );
        (uint256 startIndex, uint256 pageLength) = getPageStartAndLength(
            account.actionHashesList.length(),
            pageNumber,
            pageSize
        );

        AppStorage.Metadata[] memory pageMetadata = new AppStorage.Metadata[](
            pageLength
        );

        for (uint256 i = 0; i < pageLength; i++) {
            pageMetadata[i] = account.actionMetadata[
                account.actionHashesList.at(startIndex + i)
            ];
        }
        return pageMetadata;
    }

    function listActionsInGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (AppStorage.Metadata[] memory) {
        AppStorage.Account storage account = getReadOnlyAccount(
            accountApiKeyHash
        );
        AppStorage.Group storage group = account.groups[groupId];
        uint256 totalLength = group.cidHash.length();
        (uint256 startIndex, uint256 pageLength) = getPageStartAndLength(
            totalLength,
            pageNumber,
            pageSize
        );

        AppStorage.Metadata[] memory pageMetadata = new AppStorage.Metadata[](
            pageLength
        );
        for (uint256 i = 0; i < pageLength; i++) {
            pageMetadata[i] = account.actionMetadata[
                group.cidHash.at(startIndex + i)
            ];
        }
        return pageMetadata;
    }

    function getPageStartAndLength(
        uint256 totalLength,
        uint256 pageNumber,
        uint256 pageSize
    ) internal pure returns (uint256, uint256) {
        if (pageSize > totalLength) {
            pageSize = totalLength;
            pageNumber = 0;
        }
        uint256 startIndex = pageNumber * pageSize;
        if (startIndex >= totalLength) {
            return (0, 0);
        }
        uint256 endIndex = startIndex + pageSize;
        if (endIndex > totalLength) endIndex = totalLength;
        return (startIndex, endIndex - startIndex);
    }

    // Internal function to get the account for a given api key hash and sender
    // @notice Reverts if the account does not exist or the sender is not allowed to access it
    // @notice The API Key, could be a usage or group API key
    // @param accountApiKeyHash The keccak256 hash of the account api key
    // @param sender The address of the sender
    // @return The account
    function getReadOnlyAccount(
        uint256 apiKeyHash
    ) internal view returns (AppStorage.Account storage) {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        uint256 masterAccountApiKeyHash = s.allApiKeyHashesToMaster[apiKeyHash];
        if (masterAccountApiKeyHash == 0) {
            revert AppStorage.AccountDoesNotExist(apiKeyHash);
        }
        AppStorage.Account storage account = s.accounts[
            masterAccountApiKeyHash
        ];
        return account;
    }

    function canExecuteAction(
        uint256 apiKeyHash,
        uint256 cidHash
    ) public view returns (bool) {
        uint256[] memory groupIds = groupIdsForAction(apiKeyHash, cidHash);
        return apiKeyCanExecuteForAnyGroup(apiKeyHash, groupIds);
    }

    function canUseWalletInAction(
        uint256 apiKeyHash,
        uint256 cidHash,
        address walletAddress
    ) public view returns (bool) {
        uint256[] memory groupIds = groupIdsForActionAndWallet(
            apiKeyHash,
            cidHash,
            walletAddress
        );
        return apiKeyCanExecuteForAnyGroup(apiKeyHash, groupIds);
    }

    function apiKeyCanExecuteForAnyGroup(
        uint256 apiKeyHash,
        uint256[] memory groupIds
    ) public view returns (bool) {
        AppStorage.Account storage account = getReadOnlyAccount(apiKeyHash);
        AppStorage.UsageApiKey storage usageApiKey = account.usageApiKeys[
            apiKeyHash
        ];

        //  wildcard scenario
        if (usageApiKey.executeInGroups.contains(0)) {
            return true;
        }

        for (uint256 i = 0; i < groupIds.length; i++) {
            if (usageApiKey.executeInGroups.contains(groupIds[i])) {
                return true;
            }
        }
        return false;
    }

    function groupIdsForAction(
        uint256 apiKeyHash,
        uint256 cidHash
    ) public view returns (uint256[] memory) {
        AppStorage.Account storage account = getReadOnlyAccount(apiKeyHash);
        uint256[] memory groupIds = new uint256[](account.groupList.length());
        uint256 groupCount = 0;
        for (uint256 i = 0; i < account.groupList.length(); i++) {
            AppStorage.Group storage group = account.groups[
                account.groupList.at(i)
            ];
            if (group.cidHash.contains(cidHash) || group.cidHash.contains(0)) {
                groupIds[groupCount] = account.groupList.at(i);
                groupCount++;
            }
        }
        uint256[] memory returnGroups = new uint256[](groupCount);
        for (uint256 i = 0; i < groupCount; i++) {
            returnGroups[i] = groupIds[i];
        }
        return returnGroups;
    }

    function groupIdsForActionAndWallet(
        uint256 apiKeyHash,
        uint256 cidHash,
        address walletAddress
    ) public view returns (uint256[] memory) {
        AppStorage.Account storage account = getReadOnlyAccount(apiKeyHash);
        uint256[] memory groupIds = new uint256[](account.groupList.length());
        uint256 groupCount = 0;
        for (uint256 i = 0; i < account.groupList.length(); i++) {
            AppStorage.Group storage group = account.groups[
                account.groupList.at(i)
            ];
            if (
                (group.cidHash.contains(cidHash) ||
                    group.cidHash.contains(0)) &&
                (group.pkpId.contains(walletAddress) ||
                    group.pkpId.contains(address(0)))
            ) {
                groupIds[groupCount] = account.groupList.at(i);
                groupCount++;
            }
        }
        uint256[] memory returnGroups = new uint256[](groupCount);
        for (uint256 i = 0; i < groupCount; i++) {
            returnGroups[i] = groupIds[i];
        }
        return returnGroups;
    }

    /// @notice Optimized single-pass canExecuteAction with early exit. No intermediate array.
    function canExecuteActionFast(
        uint256 apiKeyHash,
        uint256 cidHash
    ) public view returns (bool) {
        AppStorage.Account storage account = getReadOnlyAccount(apiKeyHash);
        AppStorage.UsageApiKey storage usageApiKey = account.usageApiKeys[
            apiKeyHash
        ];

        if (usageApiKey.executeInGroups.contains(0)) {
            return true; // wildcard: can execute in any group
        }

        uint256 len = account.groupList.length();
        for (uint256 i = 0; i < len; i++) {
            uint256 groupId = account.groupList.at(i);
            AppStorage.Group storage group = account.groups[groupId];
            if (
                (group.cidHash.contains(cidHash) ||
                    group.cidHash.contains(0)) &&
                usageApiKey.executeInGroups.contains(groupId)
            ) {
                return true;
            }
        }
        return false;
    }

    /// @notice Optimized single-pass canUseWalletInAction with early exit. No intermediate array.
    function canUseWalletInActionFast(
        uint256 apiKeyHash,
        uint256 cidHash,
        address walletAddress
    ) public view returns (bool) {
        AppStorage.Account storage account = getReadOnlyAccount(apiKeyHash);
        AppStorage.UsageApiKey storage usageApiKey = account.usageApiKeys[
            apiKeyHash
        ];

        if (usageApiKey.executeInGroups.contains(0)) {
            return true; // wildcard
        }

        uint256 len = account.groupList.length();
        for (uint256 i = 0; i < len; i++) {
            uint256 groupId = account.groupList.at(i);
            AppStorage.Group storage group = account.groups[groupId];
            if (
                (group.cidHash.contains(cidHash) ||
                    group.cidHash.contains(0)) &&
                (group.pkpId.contains(walletAddress) ||
                    group.pkpId.contains(address(0))) &&
                usageApiKey.executeInGroups.contains(groupId)
            ) {
                return true;
            }
        }
        return false;
    }

    /// @notice Combined check: returns (canExecute, canUseWallet) in a single pass.
    ///         Use this from Rust to replace 2 sequential RPC calls with 1.
    ///         Note: canUseWallet implies canExecute in this model.
    function canExecuteActionAndUseWallet(
        uint256 apiKeyHash,
        uint256 cidHash,
        address walletAddress
    ) public view returns (bool canExecute, bool canUseWallet) {
        AppStorage.Account storage account = getReadOnlyAccount(apiKeyHash);
        AppStorage.UsageApiKey storage usageApiKey = account.usageApiKeys[
            apiKeyHash
        ];

        if (usageApiKey.executeInGroups.contains(0)) {
            return (true, true); // wildcard: both trivially true
        }

        uint256 len = account.groupList.length();
        for (uint256 i = 0; i < len; i++) {
            if (canExecute && canUseWallet) break; // early exit once both resolved

            uint256 groupId = account.groupList.at(i);
            AppStorage.Group storage group = account.groups[groupId];

            bool cidMatch = group.cidHash.contains(cidHash) ||
                group.cidHash.contains(0);
            if (!cidMatch) continue;

            bool groupPermitted = usageApiKey.executeInGroups.contains(groupId);
            if (!groupPermitted) continue;

            canExecute = true;

            if (!canUseWallet) {
                if (
                    group.pkpId.contains(walletAddress) ||
                    group.pkpId.contains(address(0))
                ) {
                    canUseWallet = true;
                }
            }
        }
    }
}
