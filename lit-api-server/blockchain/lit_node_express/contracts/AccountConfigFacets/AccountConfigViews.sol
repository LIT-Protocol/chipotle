/// @title AccountConfigViews
/// @author Brendon Paul
/// @notice View (read-only) functions for AccountConfig diamond.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {LibAccountConfigStorage} from "./LibAccountConfigStorage.sol";

contract AccountConfigViews {
    using EnumerableSet for EnumerableSet.UintSet;
    /// @notice Getters for public state (ABI compatibility with original AccountConfig).
    function api_payer() public view returns (address) {
        return LibAccountConfigStorage.getStorage().api_payer;
    }

    function readOnlyApiHelper() internal view returns (address) {
        return LibAccountConfigStorage.getStorage().api_payer;
    }

    function pricing_operator() public view returns (address) {
        return LibAccountConfigStorage.getStorage().pricing_operator;
    }

    function owner() public view returns (address) {
        return LibAccountConfigStorage.getStorage().owner;
    }

    function nextWalletCount() public view returns (uint256) {
        return LibAccountConfigStorage.getStorage().nextWalletCount;
    }
    function nextAccountCount() public view returns (uint256) {
        return LibAccountConfigStorage.getStorage().nextAccountCount;
    }
    function indexToAccountHashAt(uint256 index) public view returns (uint256) {
        return LibAccountConfigStorage.getStorage().indexToAccountHash[index];
    }
    function allWalletAddressesAt(uint256 index) public view returns (address) {
        return LibAccountConfigStorage.getStorage().allWalletAddresses[index];
    }
    function pricingAt(uint256 index) public view returns (uint256) {
        return LibAccountConfigStorage.getStorage().pricing[index];
    }

    function accountExistsAndIsMutable(
        uint256 apiKeyHash
    ) public view returns (bool) {
        return
            LibAccountConfigStorage.accountExistsAndIsMutable(
                apiKeyHash,
                msg.sender
            );
    }

    function getWalletDerivation(
        uint256 apiKeyHash,
        address walletAddress
    ) public view returns (uint256) {
        LibAccountConfigStorage.Account storage account = getReadOnlyAccount(
            apiKeyHash
        );
        return account.walletData[walletAddress].id;
    }

    function listApiKeys(
        uint256 accountApiKeyHash,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (LibAccountConfigStorage.UsageApiKey[] memory) {
        LibAccountConfigStorage.Account storage account = getReadOnlyAccount(
            accountApiKeyHash
        );
        uint256 totalLength = account.usageApiKeysList.length();
        (uint256 startIndex, uint256 pageLength) = getPageStartAndLength(
            totalLength,
            pageNumber,
            pageSize
        );
        LibAccountConfigStorage.UsageApiKey[]
            memory pageApiKeys = new LibAccountConfigStorage.UsageApiKey[](
                pageLength
            );
        for (uint256 i = 0; i < pageLength; i++) {
            pageApiKeys[i] = account.usageApiKeys[
                account.usageApiKeysList.at(startIndex + i)
            ];
        }
        return pageApiKeys;
    }

    function listGroups(
        uint256 accountApiKeyHash,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (LibAccountConfigStorage.Metadata[] memory) {
        LibAccountConfigStorage.Account storage account = getReadOnlyAccount(
            accountApiKeyHash
        );
        uint256 totalLength = account.groupList.length();
        (uint256 startIndex, uint256 pageLength) = getPageStartAndLength(
            totalLength,
            pageNumber,
            pageSize
        );
        LibAccountConfigStorage.Metadata[]
            memory pageMetadata = new LibAccountConfigStorage.Metadata[](
                pageLength
            );
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
    ) public view returns (LibAccountConfigStorage.WalletData[] memory) {
        LibAccountConfigStorage.Account storage account = getReadOnlyAccount(
            accountApiKeyHash
        );
        (uint256 startIndex, uint256 pageLength) = getPageStartAndLength(
            account.walletCount,
            pageNumber,
            pageSize
        );

        LibAccountConfigStorage.WalletData[]
            memory pageWalletData = new LibAccountConfigStorage.WalletData[](
                pageLength
            );
        for (uint256 i = 0; i < pageLength; i++) {
            address walletAddress = account.walletAddresses[startIndex + i];
            pageWalletData[i].walletAddress = walletAddress;
            pageWalletData[i].name = account.walletData[walletAddress].name;
            pageWalletData[i].description = account
                .walletData[walletAddress]
                .description;
            pageWalletData[i].id = account.walletData[walletAddress].id;
        }
        return pageWalletData;
    }

    function listWalletsInGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (LibAccountConfigStorage.WalletData[] memory) {
        LibAccountConfigStorage.Account storage account = getReadOnlyAccount(
            accountApiKeyHash
        );
        LibAccountConfigStorage.AccountConfigStorage storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Group storage group = account.groups[groupId];
        (uint256 startIndex, uint256 pageLength) = getPageStartAndLength(
            group.Wallets_hash.length(),
            pageNumber,
            pageSize
        );
        LibAccountConfigStorage.WalletData[]
            memory pageWalletData = new LibAccountConfigStorage.WalletData[](
                pageLength
            );
        for (uint256 i = 0; i < pageLength; i++) {
            uint256 wallet_hash = group.Wallets_hash.at(startIndex + i);
            address walletAddress = s.allWalletAddresses[wallet_hash];
            pageWalletData[i].walletAddress = walletAddress;
            pageWalletData[i].name = account.walletData[walletAddress].name;
            pageWalletData[i].description = account
                .walletData[walletAddress]
                .description;
            pageWalletData[i].id = account.walletData[walletAddress].id;
        }
        return pageWalletData;
    }

    function listActions(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (LibAccountConfigStorage.Metadata[] memory) {
        LibAccountConfigStorage.Account storage account = getReadOnlyAccount(
            accountApiKeyHash
        );
        LibAccountConfigStorage.Group storage group = account.groups[groupId];
        uint256 totalLength = group.permitted_actions_cid_hash.length();
        (uint256 startIndex, uint256 pageLength) = getPageStartAndLength(
            totalLength,
            pageNumber,
            pageSize
        );

        LibAccountConfigStorage.Metadata[]
            memory pageMetadata = new LibAccountConfigStorage.Metadata[](
                pageLength
            );
        for (uint256 i = 0; i < pageLength; i++) {
            pageMetadata[i] = account.actionMetadata[
                group.permitted_actions_cid_hash.at(startIndex + i)
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
    ) internal view returns (LibAccountConfigStorage.Account storage) {
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        uint256 masterAccountApiKeyHash = s.allApiKeyHashes[apiKeyHash];
        if (masterAccountApiKeyHash == 0) {
            revert LibAccountConfigStorage.AccountDoesNotExist(apiKeyHash);
        }
        LibAccountConfigStorage.Account storage account = s.accounts[
            masterAccountApiKeyHash
        ];
        return account;
    }

    function getPricing(uint256 pricingItemId) public view returns (uint256) {
        return LibAccountConfigStorage.getStorage().pricing[pricingItemId];
    }
}
