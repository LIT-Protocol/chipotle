/// @title AccountConfigWrite
/// @author Brendon Paul
/// @notice Mutable (state-changing) functions for AccountConfig diamond.

// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {AppStorage} from "./AppStorage.sol";
import {SecurityLib} from "./SecurityLib.sol";

contract WritesFacet {
    using EnumerableSet for EnumerableSet.UintSet;
    using EnumerableSet for EnumerableSet.AddressSet;
    using EnumerableSet for EnumerableSet.StringSet;

    event AccountCreated(
        uint256 indexed apiKeyHash,
        address indexed creator,
        bool managed
    );
    event UsageApiKeySet(
        uint256 indexed accountApiKeyHash,
        uint256 indexed usageApiKeyHash
    );
    event GroupAdded(uint256 indexed apiKeyHash, uint256 indexed groupId);
    event GroupUpdated(
        uint256 indexed accountApiKeyHash,
        uint256 indexed groupId
    );
    event GroupRemoved(uint256 indexed apiKeyHash, uint256 indexed groupId);
    event ActionAdded(
        uint256 indexed accountApiKeyHash,
        uint256 indexed actionHash
    );
    event ActionRemoved(
        uint256 indexed accountApiKeyHash,
        uint256 indexed actionHash
    );
    event PkpAddedToGroup(
        uint256 indexed apiKeyHash,
        uint256 indexed groupId,
        address pkpId
    );
    event PkpRemovedFromGroup(
        uint256 indexed apiKeyHash,
        uint256 indexed groupId,
        address pkpId
    );
    event ActionAddedToGroup(
        uint256 indexed apiKeyHash,
        uint256 indexed groupId,
        uint256 action
    );
    event ActionRemovedFromGroup(
        uint256 indexed apiKeyHash,
        uint256 indexed groupId,
        uint256 action
    );
    event WalletDerivationRegistered(
        uint256 indexed apiKeyHash,
        address indexed pkpId,
        uint256 derivationPath
    );
    event UsageApiKeyRemoved(
        uint256 indexed accountApiKeyHash,
        uint256 indexed usageApiKeyHash
    );

    function newChainSecuredAccount(
        string memory accountName,
        string memory accountDescription,
    ) public {
        address adminWalletAddress = msg.sender;
        uint256 apiKeyHash = keccak256(abi.encodePacked(adminWalletAddress));
        newAccount(apiKeyHash, false, accountName, accountDescription, adminWalletAddress);
    }

    function newAccount(
        uint256 apiKeyHash,
        bool managed,
        string memory accountName,
        string memory accountDescription,
        address adminWalletAddress
    ) public {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (s.allApiKeyHashesToMaster[apiKeyHash] != 0) {
            revert AppStorage.AccountAlreadyExists(apiKeyHash);
        }
        AppStorage.Account storage account = s.accounts[apiKeyHash];
        account.managed = managed;
        account.adminWalletAddress = adminWalletAddress;
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
        emit AccountCreated(apiKeyHash, adminWalletAddress, managed);
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
        if (manageIPFSIdsInGroups.length > 50) {
            revert AppStorage.InvalidRequest(
                "manageIPFSIdsInGroups must be 50 items or fewer"
            );
        }
        if (addPkpToGroups.length > 50) {
            revert AppStorage.InvalidRequest(
                "addPkpToGroups must be 50 items or fewer"
            );
        }
        if (removePkpFromGroups.length > 50) {
            revert AppStorage.InvalidRequest(
                "removePkpFromGroups must be 50 items or fewer"
            );
        }
        if (executeInGroups.length > 50) {
            revert AppStorage.InvalidRequest(
                "executeInGroups must be 50 items or fewer"
            );
        }
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
        emit UsageApiKeySet(masterAccountApiKeyHash, usageApiKeyHash);
    }

    function addGroup(
        uint256 apiKeyHash,
        string memory name,
        string memory description,
        uint256[] memory cidHashes,
        address[] memory pkpIds
    ) public returns (uint256) {
        if (cidHashes.length > 10) {
            revert AppStorage.InvalidRequest(
                "cidHashes must be 10 items or fewer"
            );
        }
        if (pkpIds.length > 10) {
            revert AppStorage.InvalidRequest(
                "pkpIds must be 10 items or fewer"
            );
        }
        SecurityLib.revertIfNoAccountAccess(apiKeyHash, msg.sender);
        uint256 masterHash = SecurityLib.resolveToMaster(apiKeyHash);
        if (masterHash != apiKeyHash) {
            SecurityLib.canCreateGroup(apiKeyHash);
        }
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[masterHash];
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
        emit GroupAdded(masterHash, account.groupCount);
        return account.groupCount;
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
                "cidHashes must be 10 items or fewer"
            );
        }
        if (pkpIds.length > 10) {
            revert AppStorage.InvalidRequest(
                "pkpIds must be 10 items or fewer"
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
        emit GroupUpdated(accountApiKeyHash, groupId);
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

    function removeGroup(uint256 apiKeyHash, uint256 groupId) public {
        SecurityLib.revertIfNoAccountAccess(apiKeyHash, msg.sender);
        uint256 masterHash = SecurityLib.resolveToMaster(apiKeyHash);
        if (masterHash != apiKeyHash) {
            SecurityLib.canDeleteGroup(apiKeyHash);
        }
        AppStorage.revertIfGroupDoesNotExist(masterHash, groupId);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[masterHash];
        account.groupList.remove(groupId);
        delete account.groups[groupId];
        emit GroupRemoved(masterHash, groupId);
    }

    function addPkpToGroup(
        uint256 apiKeyHash,
        uint256 groupId,
        address pkpId
    ) public {
        SecurityLib.revertIfNoAccountAccess(apiKeyHash, msg.sender);
        uint256 masterHash = SecurityLib.resolveToMaster(apiKeyHash);
        if (masterHash != apiKeyHash) {
            SecurityLib.canAccountAddPkpToGroup(apiKeyHash, groupId);
        }
        AppStorage.revertIfGroupDoesNotExist(masterHash, groupId);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.accounts[masterHash].groups[groupId].pkpId.add(pkpId);
        emit PkpAddedToGroup(masterHash, groupId, pkpId);
    }

    function addAction(
        uint256 accountApiKeyHash,
        string memory name,
        string memory description,
        uint256 actionHash
    ) public {
        SecurityLib.revertIfNoAccountAccess(accountApiKeyHash, msg.sender);
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[accountApiKeyHash];
        bool added = account.actionHashesList.add(actionHash);
        if (added) {
            account.actionCount++;
        }
        account.actionMetadata[actionHash].id = actionHash;
        account.actionMetadata[actionHash].name = name;
        account.actionMetadata[actionHash].description = description;
        emit ActionAdded(accountApiKeyHash, actionHash);
    }

    function removeAction(
        uint256 accountApiKeyHash,
        uint256 actionHash
    ) public {
        if (actionHash == 0) {
            revert AppStorage.InvalidRequest(
                "Cannot remove action with hash 0x0"
            );
        }
        SecurityLib.revertIfNoAccountAccess(accountApiKeyHash, msg.sender);
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[accountApiKeyHash];

        bool removed = account.actionHashesList.remove(actionHash);
        if (removed && account.actionCount > 0) {
            account.actionCount--;
        }

        // Remove the action from all groups that may reference it to avoid stale cidHash entries.
        uint256 groupCount = account.groupList.length();
        for (uint256 i = 0; i < groupCount; i++) {
            uint256 groupId = account.groupList.at(i);
            account.groups[groupId].cidHash.remove(actionHash);
        }
        delete account.actionMetadata[actionHash];
        emit ActionRemoved(accountApiKeyHash, actionHash);
    }

    function addActionToGroup(
        uint256 apiKeyHash,
        uint256 groupId,
        uint256 action
    ) public {
        SecurityLib.revertIfNoAccountAccess(apiKeyHash, msg.sender);
        uint256 masterHash = SecurityLib.resolveToMaster(apiKeyHash);
        if (masterHash != apiKeyHash) {
            SecurityLib.canAccountManageIPFSIdsInGroup(apiKeyHash, groupId);
        }
        AppStorage.revertIfGroupDoesNotExist(masterHash, groupId);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.accounts[masterHash].groups[groupId].cidHash.add(action);
        emit ActionAddedToGroup(masterHash, groupId, action);
    }

    function updateActionMetadata(
        uint256 accountApiKeyHash,
        uint256 actionHash,
        uint256 groupId,
        string memory name,
        string memory description
    ) public {
        SecurityLib.revertIfNoAccountAccess(accountApiKeyHash, msg.sender);
        SecurityLib.revertIfNotMasterAccount(accountApiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[accountApiKeyHash];
        if (!account.actionHashesList.contains(actionHash)) {
            revert AppStorage.ActionDoesNotExist(
                accountApiKeyHash,
                groupId,
                actionHash
            );
        }
        account.actionMetadata[actionHash].name = name;
        account.actionMetadata[actionHash].description = description;
    }

    function removeActionFromGroup(
        uint256 apiKeyHash,
        uint256 groupId,
        uint256 action
    ) public {
        SecurityLib.revertIfNoAccountAccess(apiKeyHash, msg.sender);
        uint256 masterHash = SecurityLib.resolveToMaster(apiKeyHash);
        if (masterHash != apiKeyHash) {
            SecurityLib.canAccountManageIPFSIdsInGroup(apiKeyHash, groupId);
        }
        AppStorage.revertIfGroupDoesNotExist(masterHash, groupId);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[masterHash];
        if (!account.groups[groupId].cidHash.contains(action)) {
            revert AppStorage.ActionDoesNotExist(masterHash, groupId, action);
        }
        account.groups[groupId].cidHash.remove(action);
        emit ActionRemovedFromGroup(masterHash, groupId, action);
    }

    function removePkpFromGroup(
        uint256 apiKeyHash,
        uint256 groupId,
        address pkpId
    ) public {
        SecurityLib.revertIfNoAccountAccess(apiKeyHash, msg.sender);
        uint256 masterHash = SecurityLib.resolveToMaster(apiKeyHash);
        if (masterHash != apiKeyHash) {
            SecurityLib.canAccountRemovePkpFromGroup(apiKeyHash, groupId);
        }
        AppStorage.revertIfGroupDoesNotExist(masterHash, groupId);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (!s.accounts[masterHash].groups[groupId].pkpId.contains(pkpId)) {
            revert AppStorage.PkpDoesNotExist(masterHash, groupId, pkpId);
        }
        s.accounts[masterHash].groups[groupId].pkpId.remove(pkpId);
        emit PkpRemovedFromGroup(masterHash, groupId, pkpId);
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
        emit UsageApiKeyRemoved(accountApiKeyHash, usageApiKeyHash);
    }

    function registerWalletDerivation(
        uint256 apiKeyHash,
        address pkpId,
        uint256 derivationPath,
        string memory name,
        string memory description
    ) public {
        SecurityLib.revertIfNoAccountAccess(apiKeyHash, msg.sender);
        uint256 masterHash = SecurityLib.resolveToMaster(apiKeyHash);
        if (masterHash != apiKeyHash) {
            SecurityLib.canCreatePkp(apiKeyHash);
        }
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[masterHash];
        if (derivationPath == 0) {
            revert AppStorage.InvalidRequest("derivationPath must be non-zero");
        }
        if (account.pkpData[pkpId].id != 0) {
            revert AppStorage.InvalidRequest("PKP already registered");
        }
        account.pkpData[pkpId].id = derivationPath;
        account.pkpData[pkpId].name = name;
        account.pkpData[pkpId].description = description;
        account.pkpIds[account.pkpCount] = pkpId;
        account.pkpCount++;
        s.pkpCount++;
        s.allPkpIds[s.pkpCount] = pkpId;
        emit WalletDerivationRegistered(masterHash, pkpId, derivationPath);
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
