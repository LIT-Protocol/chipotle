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
    error PkpDoesNotExist(uint256 apiKey, uint256 groupId, uint256 pkp);
    error ActionDoesNotExist(uint256 apiKey, uint256 groupId, uint256 action);
    error AccountApiKeyDoesNotExist(uint256 apiKey, uint256 accountApiKey);

    struct ApiKey {
        uint256 apiKeyHash; // keccak256 of the base64 encoded api key
        uint256 expiration; // expiration date of the api key
        uint256 balance; // balance of the api key - funds that it holds for payment?
    }

    struct Group {
        uint256 groupId; // id of the group
        EnumerableSet.UintSet permitted_actions; // keccak256 of an action ipfs cid
        EnumerableSet.UintSet pkps; // keccak256 of a pkp public key
    }

    struct Account {
        uint256 apiKey; // keccak256 of the api key
        mapping(uint256 => ApiKey) apiKeys; // mapping from a keccak256 of an api key to it's config
        EnumerableSet.UintSet group_list; // set of groups that the account is a member of
        mapping(uint256 => Group) groups; // mapping from an index of a group to it's config
        bool managed; // whether the LIT-node can help manage the key.
        uint256 nextGroupId; // counter for creating unique group ids
    }

    mapping(uint256 => Account) accounts; // mapping from a given api key to it's config
    uint256 public nextPkpId; // counter for creating unique PKPs
    
    constructor() {
        nextPkpId = 0;
    }

    function newAccount(uint256 apiKey, bool managed) public {
        Account storage account = accounts[apiKey];
        account.apiKey = apiKey;
        account.managed = managed;
    }

    function accountExists(uint256 apiKey) private view returns (bool) {
        return accounts[apiKey].apiKey == apiKey;
    }

    function groupExists(uint256 apiKey, uint256 groupId) private view returns (bool) {
        if (!accountExists(apiKey)) {
            revert AccountDoesNotExist(apiKey);
        }
        return accounts[apiKey].groups[groupId].groupId == groupId;
    }

    function actionExists(uint256 apiKey, uint256 groupId, uint256 action) private view returns (bool) {
        if (!groupExists(apiKey, groupId)) {
            revert GroupDoesNotExist(apiKey, groupId);
        }
        return accounts[apiKey].groups[groupId].permitted_actions.contains(action);
    }

    function pkpExists(uint256 apiKey, uint256 groupId, uint256 pkp) private view returns (bool) {
        if (!groupExists(apiKey, groupId)) {
            revert GroupDoesNotExist(apiKey, groupId);
        }
        return accounts[apiKey].groups[groupId].pkps.contains(pkp);
    }

    function addApiKey(uint256 apiKey, uint256 expiration, uint256 balance) public {
        if (!accountExists(apiKey)) {
            revert AccountDoesNotExist(apiKey);
        }
                
        ApiKey storage apiKeyStorage = accounts[apiKey].apiKeys[apiKey];
        apiKeyStorage.apiKeyHash = apiKey;
        apiKeyStorage.expiration = expiration;
        apiKeyStorage.balance = balance;
    }

    function addGroup(uint256 apiKey, uint256[] memory permitted_actions, uint256[] memory pkps) public {
        if (!accountExists(apiKey)) {
            revert AccountDoesNotExist(apiKey);
        }

        Account storage account = accounts[apiKey];

        account.group_list.add(account.nextGroupId);
        Group storage group = account.groups[account.nextGroupId];
        group.groupId = account.nextGroupId;
        for (uint256 i = 0; i < permitted_actions.length; i++) {
            group.permitted_actions.add(permitted_actions[i]);
        }
        for (uint256 i = 0; i < pkps.length; i++) {
            group.pkps.add(pkps[i]);
        }
        account.nextGroupId++;
    }

    function addPkpToGroup(uint256 apiKey, uint256 groupId, uint256 pkp) public {        
        // this checks for the account and group existence
        if (!groupExists(apiKey, groupId)) { 
            revert GroupDoesNotExist(apiKey, groupId);
        }

        Account storage account = accounts[apiKey];
        account.groups[groupId].pkps.add(pkp);
    }

    function addActionToGroup(uint256 apiKey, uint256 groupId, uint256 action) public {
        if (!groupExists(apiKey, groupId)) {
            revert GroupDoesNotExist(apiKey, groupId);
        }

        Account storage account = accounts[apiKey];
        account.groups[groupId].permitted_actions.add(action);
    }

    function removeActionFromGroup(uint256 apiKey, uint256 groupId, uint256 action) public {
        if (!actionExists(apiKey, groupId, action)) {
            revert ActionDoesNotExist(apiKey, groupId, action);
        }

        Account storage account = accounts[apiKey];
        account.groups[groupId].permitted_actions.remove(action);
    }

    function removePkpFromGroup(uint256 apiKey, uint256 groupId, uint256 pkp) public {
        if (!pkpExists(apiKey, groupId, pkp)) {
            revert PkpDoesNotExist(apiKey, groupId, pkp);
        }

        Account storage account = accounts[apiKey];
        account.groups[groupId].pkps.remove(pkp);
    }

    function removeAccountApiKey(uint256 apiKey, uint256 accountApiKey) public {
        if (!accountApiKeyExists(apiKey, accountApiKey)) {
            revert AccountApiKeyDoesNotExist(apiKey, accountApiKey);
        }

        Account storage account = accounts[apiKey];
        delete account.apiKeys[accountApiKey];
    }

    function accountApiKeyExists(uint256 apiKey, uint256 accountApiKey) private view returns (bool) {        
        if (!accountExists(apiKey)) {
            revert AccountDoesNotExist(apiKey);
        }
        return accounts[apiKey].apiKeys[accountApiKey].apiKeyHash == accountApiKey;
    }
}
