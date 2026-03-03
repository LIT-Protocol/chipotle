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

    function pricing_operator() public view returns (address) {
        return LibAccountConfigStorage.getStorage().pricing_operator;
    }

    function nextWalletCount() public view returns (uint256) {
        return LibAccountConfigStorage.getStorage().nextWalletCount;
    }

    function accountExistsAndIsMutable(
        uint256 apiKeyHash
    ) public view returns (bool) {
        return
            LibAccountConfigStorage.accountExistsAndIsMutable(
                LibAccountConfigStorage.getStorage(),
                apiKeyHash,
                msg.sender
            );
    }

    function getWalletDerivation(
        uint256 apiKeyHash,
        uint256 walletAddressHash
    ) public view returns (uint256) {
        LibAccountConfigStorage.revertIfAccountDoesNotExistAndIsMutable(
            LibAccountConfigStorage.getStorage(),
            apiKeyHash,
            msg.sender
        );
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        uint256 masterAccountApiKeyHash = s.allApiKeyHashes[apiKeyHash];
        LibAccountConfigStorage.Account storage account = s.accounts[
            masterAccountApiKeyHash
        ];
        return account.wallet_derivation[walletAddressHash];
    }

    function listApiKeys(
        uint256 accountApiKeyHash,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (LibAccountConfigStorage.UsageApiKey[] memory) {
        LibAccountConfigStorage.revertIfAccountDoesNotExistAndIsMutable(
            LibAccountConfigStorage.getStorage(),
            accountApiKeyHash,
            msg.sender
        );
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Account storage account = s.accounts[
            accountApiKeyHash
        ];
        uint256 pageLength = account.usageApiKeysList.length();
        if (pageSize > pageLength) {
            pageSize = pageLength;
            pageNumber = 0;
        }
        uint256 startIndex = pageNumber * pageSize;
        uint256 endIndex = startIndex + pageSize;
        if (endIndex > pageLength) endIndex = pageLength;
        uint256 resultLength = endIndex - startIndex;
        LibAccountConfigStorage.UsageApiKey[]
            memory pageApiKeys = new LibAccountConfigStorage.UsageApiKey[](
                resultLength
            );
        for (uint256 i = 0; i < resultLength; i++) {
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
        LibAccountConfigStorage.revertIfAccountDoesNotExistAndIsMutable(
            LibAccountConfigStorage.getStorage(),
            accountApiKeyHash,
            msg.sender
        );
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Account storage account = s.accounts[
            accountApiKeyHash
        ];
        uint256 totalLength = account.groupList.length();
        if (totalLength == 0) {
            return new LibAccountConfigStorage.Metadata[](0);
        }
        if (pageSize > totalLength) {
            pageSize = totalLength;
            pageNumber = 0;
        }
        uint256 startIndex = pageNumber * pageSize;
        if (startIndex >= totalLength) {
            return new LibAccountConfigStorage.Metadata[](0);
        }
        uint256 endIndex = startIndex + pageSize;
        if (endIndex > totalLength) endIndex = totalLength;
        uint256 pageLength = endIndex - startIndex;
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
    ) public view returns (LibAccountConfigStorage.Metadata[] memory) {
        LibAccountConfigStorage.revertIfAccountDoesNotExistAndIsMutable(
            LibAccountConfigStorage.getStorage(),
            accountApiKeyHash,
            msg.sender
        );
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Account storage account = s.accounts[
            accountApiKeyHash
        ];
        if (pageSize > account.walletCount) {
            pageSize = account.walletCount;
            pageNumber = 0;
        }
        uint256 startIndex = pageNumber * pageSize;
        uint256 endIndex = startIndex + pageSize;
        if (endIndex > account.walletCount) endIndex = account.walletCount;
        uint256 pageLength = endIndex - startIndex;
        LibAccountConfigStorage.Metadata[]
            memory pageMetadata = new LibAccountConfigStorage.Metadata[](
                pageLength
            );
        for (uint256 i = 0; i < pageLength; i++) {
            pageMetadata[i] = account.walletDerivationMetadata[
                account.walletAddressHash[startIndex + i]
            ];
        }
        return pageMetadata;
    }

    function listWalletsInGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 pageNumber,
        uint256 pageSize
    ) public view returns (LibAccountConfigStorage.Metadata[] memory) {
        LibAccountConfigStorage.revertIfAccountDoesNotExistAndIsMutable(
            LibAccountConfigStorage.getStorage(),
            accountApiKeyHash,
            msg.sender
        );
        LibAccountConfigStorage.revertIfGroupDoesNotExist(
            LibAccountConfigStorage.getStorage(),
            accountApiKeyHash,
            groupId
        );
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Account storage account = s.accounts[
            accountApiKeyHash
        ];
        LibAccountConfigStorage.Group storage group = account.groups[groupId];
        if (pageSize > group.Wallets_hash.length()) {
            pageSize = group.Wallets_hash.length();
            pageNumber = 0;
        }
        uint256 startIndex = pageNumber * pageSize;
        uint256 endIndex = startIndex + pageSize;
        if (endIndex > group.Wallets_hash.length())
            endIndex = group.Wallets_hash.length();
        uint256 pageLength = endIndex - startIndex;
        LibAccountConfigStorage.Metadata[]
            memory pageMetadata = new LibAccountConfigStorage.Metadata[](
                pageLength
            );
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
    ) public view returns (LibAccountConfigStorage.Metadata[] memory) {
        LibAccountConfigStorage.revertIfAccountDoesNotExistAndIsMutable(
            LibAccountConfigStorage.getStorage(),
            accountApiKeyHash,
            msg.sender
        );
        LibAccountConfigStorage.revertIfGroupDoesNotExist(
            LibAccountConfigStorage.getStorage(),
            accountApiKeyHash,
            groupId
        );
        LibAccountConfigStorage.AccountConfigStorage
            storage s = LibAccountConfigStorage.getStorage();
        LibAccountConfigStorage.Account storage account = s.accounts[
            accountApiKeyHash
        ];
        LibAccountConfigStorage.Group storage group = account.groups[groupId];
        if (pageSize > group.permitted_actions_cid_hash.length()) {
            pageSize = group.permitted_actions_cid_hash.length();
            pageNumber = 0;
        }
        uint256 startIndex = pageNumber * pageSize;
        uint256 endIndex = startIndex + pageSize;
        if (endIndex > group.permitted_actions_cid_hash.length())
            endIndex = group.permitted_actions_cid_hash.length();
        uint256 pageLength = endIndex - startIndex;
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

    function getPricing(uint256 pricingItemId) public view returns (uint256) {
        return LibAccountConfigStorage.getStorage().pricing[pricingItemId];
    }
}
