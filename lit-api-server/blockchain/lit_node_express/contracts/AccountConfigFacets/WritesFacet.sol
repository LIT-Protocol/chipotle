/// @title AccountConfigWrite
/// @author Brendon Paul
/// @notice Mutable (state-changing) functions for AccountConfig diamond.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {AppStorage} from "./AppStorage.sol";
import {SecurityLib} from "./SecurityLib.sol";

contract WritesFacet {
    using EnumerableSet for EnumerableSet.UintSet;
    using EnumerableSet for EnumerableSet.AddressSet;
    using EnumerableSet for EnumerableSet.StringSet;
    
    function newAccount(
        uint256 apiKeyHash,
        bool managed,
        string memory accountName,
        string memory accountDescription,
        address creatorWalletAddress
    ) public {
        SecurityLib.revertIfNotApiPayerOrOwner(msg.sender); // for now, the UI is the only one that can create accounts
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (s.allApiKeyHashesToMaster[apiKeyHash] != 0) {
            revert AppStorage.AccountAlreadyExists(apiKeyHash);
        }
        AppStorage.Account storage account = s.accounts[apiKeyHash];
        account.managed = managed;
        account.creatorWalletAddress = creatorWalletAddress;
        account.accountApiKey.metadata.id = apiKeyHash;
        account.accountApiKey.metadata.name = accountName;
        account.accountApiKey.metadata.description = accountDescription;
        account.accountApiKey.createGroups = true;
        account.accountApiKey.deleteGroups = true;
        account.accountApiKey.createPKPs = true;
        account.accountApiKey.apiKeyHash = apiKeyHash;
        account.accountApiKey.expiration = block.timestamp + 365 days * 10;
        account.accountApiKey.balance = 0;
        s.allApiKeyHashesToMaster[apiKeyHash] = apiKeyHash;
        s.accountCount++;
        s.indexToAccountHash[s.accountCount] = apiKeyHash;
    }

    function setUsageApiKey(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash,
        uint256 expiration,
        uint256 balance,
        string memory name,
        string memory description,
        bool createGroups,
        bool deleteGroups,
        bool createPKPs,
        uint256[] memory manageIPFSIdsInGroups,
        uint256[] memory addPkpToGroups,
        uint256[] memory removePkpFromGroups,
        uint256[] memory executeInGroups
    ) public {
        SecurityLib.revertIfNoAccountAccess(accountApiKeyHash, msg.sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        uint256 masterAccountApiKeyHash = s.allApiKeyHashesToMaster[
            accountApiKeyHash
        ];
        AppStorage.UsageApiKey storage apiKeyStorage = s
            .accounts[masterAccountApiKeyHash]
            .usageApiKeys[usageApiKeyHash];
        apiKeyStorage.apiKeyHash = usageApiKeyHash;
        apiKeyStorage.expiration = expiration;
        apiKeyStorage.balance = balance;
        apiKeyStorage.createGroups = createGroups;
        apiKeyStorage.deleteGroups = deleteGroups;
        apiKeyStorage.createPKPs = createPKPs;

        // clear and reload - this isn't super efficient, but should be fine for most use cases.
        apiKeyStorage.manageIPFSIdsInGroups.clear();
        apiKeyStorage.addPkpToGroups.clear();
        apiKeyStorage.removePkpFromGroups.clear();
        apiKeyStorage.executeInGroups.clear();
        for (uint256 i = 0; i < manageIPFSIdsInGroups.length; i++) {
            apiKeyStorage.manageIPFSIdsInGroups.add(manageIPFSIdsInGroups[i]);
        }
        for (uint256 i = 0; i < addPkpToGroups.length; i++) {
            apiKeyStorage.addPkpToGroups.add(addPkpToGroups[i]);
        }
        for (uint256 i = 0; i < removePkpFromGroups.length; i++) {
            apiKeyStorage.removePkpFromGroups.add(removePkpFromGroups[i]);
        }
        for (uint256 i = 0; i < executeInGroups.length; i++) {
            apiKeyStorage.executeInGroups.add(executeInGroups[i]);
        }
        apiKeyStorage.metadata.id = usageApiKeyHash;
        apiKeyStorage.metadata.name = name;
        apiKeyStorage.metadata.description = description;
        s.accounts[masterAccountApiKeyHash].usageApiKeysList.add(
            usageApiKeyHash
        );
        s.allApiKeyHashesToMaster[usageApiKeyHash] = masterAccountApiKeyHash;
    }

    function addGroup(
        uint256 accountApiKeyHash,
        string memory name,
        string memory description,
        uint256[] memory cidHashes,
        address[] memory pkpIds
    ) public {
        SecurityLib.revertIfNoAccountAccess(accountApiKeyHash, msg.sender);
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[accountApiKeyHash];
        account.groupCount++;
        account.groupList.add(account.groupCount);
        AppStorage.Group storage group = account.groups[account.groupCount];
        group.metadata.id = account.groupCount;
        group.metadata.name = name;
        group.metadata.description = description;
        for (uint256 i = 0; i < cidHashes.length; i++) {
            group.cidHash.add(cidHashes[i]);
        }
        for (uint256 i = 0; i < pkpIds.length; i++) {
            group.pkpId.add(pkpIds[i]);
        }
    }

    function updateGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        string memory name,
        string memory description,
        uint256[] memory cidHashes,
        address[] memory pkpIds
    ) public {
        if (cidHashes.length > 10) {
            revert AppStorage.InvalidRequest(
                "cidHashes must be less than 10 items to bulk update"
            );
        }
        if (pkpIds.length > 10) {
            revert AppStorage.InvalidRequest(
                "pkpIds must be less than 10 items to bulk update"
            );
        }

        SecurityLib.revertIfGroupDoesNotExist(
            accountApiKeyHash,
            groupId,
            msg.sender
        );
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Group storage group = s.accounts[accountApiKeyHash].groups[
            groupId
        ];
        group.metadata.name = name;
        group.metadata.description = description;

        // clear and reload - this isn't super efficient.
        group.cidHash.clear();
        group.pkpId.clear();
        for (uint256 i = 0; i < cidHashes.length; i++) {
            group.cidHash.add(cidHashes[i]);
        }
        
        for (uint256 i = 0; i < pkpIds.length; i++) {
            group.pkpId.add(pkpIds[i]);
        }
    }

    function updateGroupMetadata(
        uint256 accountApiKeyHash,
        uint256 groupId,
        string memory name,
        string memory description
    ) public {
        SecurityLib.revertIfGroupDoesNotExist(
            accountApiKeyHash,
            groupId,
            msg.sender
        );
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Group storage group = s.accounts[accountApiKeyHash].groups[
            groupId
        ];
        group.metadata.name = name;
        group.metadata.description = description;
    }

    function removeGroup(
        uint256 accountApiKeyHash,
        uint256 groupId
    ) public {
        SecurityLib.revertIfGroupDoesNotExist(
            accountApiKeyHash,
            groupId,
            msg.sender
        );
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[accountApiKeyHash];
        account.groupList.remove(groupId);
        delete account.groups[groupId];
    }

    function addPkpToGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        address pkpId
    ) public {
        SecurityLib.revertIfGroupDoesNotExist(
            accountApiKeyHash,
            groupId,
            msg.sender
        );
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.accounts[accountApiKeyHash].groups[groupId].pkpId.add(pkpId);
    }

    function addAction(
        uint256 accountApiKeyHash,
        string memory name,
        string memory description
    ) public {
        SecurityLib.revertIfNoAccountAccess(accountApiKeyHash, msg.sender);
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[accountApiKeyHash];
        account.actionCount++;
        uint256 actionId = account.actionCount;
        account.actionMetadata[actionId].id = actionId;
        account.actionMetadata[actionId].name = name;
        account.actionMetadata[actionId].description = description;
    }

    function addActionToGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 action
    ) public {
        SecurityLib.revertIfGroupDoesNotExist(
            accountApiKeyHash,
            groupId,
            msg.sender
        );
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.accounts[accountApiKeyHash].groups[groupId].cidHash.add(action);
    }

    function updateActionMetadata(
        uint256 accountApiKeyHash,
        uint256 actionHash,
        uint256 groupId,
        string memory name,
        string memory description
    ) public {
        SecurityLib.revertIfActionDoesNotExist(
            accountApiKeyHash,
            groupId,
            actionHash,
            msg.sender
        );
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[accountApiKeyHash];
        account.actionMetadata[actionHash].name = name;
        account.actionMetadata[actionHash].description = description;
    }

    function removeActionFromGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        uint256 action
    ) public {
        SecurityLib.revertIfActionDoesNotExist(
            accountApiKeyHash,
            groupId,
            action,
            msg.sender
        );
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[accountApiKeyHash];
        account.groups[groupId].cidHash.remove(action);
        if (account.actionCount > 0) {
            account.actionCount--;
        }
    }

    function removePkpFromGroup(
        uint256 accountApiKeyHash,
        uint256 groupId,
        address pkpId
    ) public {
        SecurityLib.revertIfNoAccountAccess(accountApiKeyHash, msg.sender);
        SecurityLib.revertIfPkpDoesNotExist(
            accountApiKeyHash,
            groupId,
            pkpId,
            msg.sender
        );
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.accounts[accountApiKeyHash].groups[groupId].pkpId.remove(pkpId);
    }

    function updateUsageApiKeyMetadata(
        uint256 accountApiKeyHash,
        uint256 usageApiKeyHash,
        string memory name,
        string memory description
    ) public {
        SecurityLib.revertIfNoAccountAccess(accountApiKeyHash, msg.sender);
        SecurityLib.revertIfUsageApiKeyDoesNotExist(
            accountApiKeyHash,
            usageApiKeyHash
        );
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
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
        SecurityLib.revertIfNoAccountAccess(accountApiKeyHash, msg.sender);
        SecurityLib.revertIfUsageApiKeyDoesNotExist(
            accountApiKeyHash,
            usageApiKeyHash
        );
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[accountApiKeyHash];
        account.usageApiKeysList.remove(usageApiKeyHash);
        delete account.usageApiKeys[usageApiKeyHash];
        delete s.allApiKeyHashesToMaster[usageApiKeyHash];
    }

    function registerWalletDerivation(
        uint256 accountApiKeyHash,
        address pkpId,
        uint256 derivationPath,
        string memory name,
        string memory description
    ) public {
        SecurityLib.revertIfNoAccountAccess(accountApiKeyHash, msg.sender);
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[accountApiKeyHash];
        account.pkpData[pkpId].id = derivationPath;
        account.pkpData[pkpId].name = name;
        account.pkpData[pkpId].description = description;
        account.pkpIds[account.pkpCount] = pkpId;
        account.pkpCount++;
        s.pkpCount++;
        s.allPkpIds[s.pkpCount] = pkpId;
    }

    function setNodeConfiguration(
        string memory key,
        string memory value
    ) public {
        SecurityLib.revertIfNotApiPayerOrOwner(msg.sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.nodeConfigurationKeys.add(key);
        s.nodeConfigurationValues[key] = value;
    }
}
