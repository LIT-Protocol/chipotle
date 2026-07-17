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
        address indexed admin,
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
    event WalletDerivationRemoved(
        uint256 indexed apiKeyHash,
        address indexed pkpId
    );
    event UsageApiKeyRemoved(
        uint256 indexed accountApiKeyHash,
        uint256 indexed usageApiKeyHash
    );
    event AccountConvertedToChainSecured(
        uint256 indexed apiKeyHash,
        address indexed newAdminWalletAddress
    );
    event ChainSecuredAccountOwnershipTransferred(
        uint256 indexed apiKeyHash,
        address indexed previousAdminWalletAddress,
        address indexed newAdminWalletAddress
    );

    function newChainSecuredAccount(
        string memory accountName,
        string memory accountDescription
    ) public {
        address adminWalletAddress = msg.sender;
        uint256 apiKeyHash = uint256(
            keccak256(abi.encodePacked(adminWalletAddress))
        );
        newAccount(
            apiKeyHash,
            false,
            accountName,
            accountDescription,
            adminWalletAddress
        );
    }

    function newAccount(
        uint256 apiKeyHash,
        bool managed,
        string memory accountName,
        string memory accountDescription,
        address adminWalletAddress
    ) public {
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (!SecurityLib.isApiPayerOrOwner(msg.sender)) {
            if (managed) {
                revert AppStorage.InvalidRequest(
                    "ChainSecured accounts must be unmanaged."
                );
            }
            if (
                apiKeyHash != uint256(keccak256(abi.encodePacked(msg.sender)))
            ) {
                revert AppStorage.InvalidRequest(
                    "ChainSecured apiKeyHash must equal the keccak256 of the sender."
                );
            }
            if (adminWalletAddress != msg.sender) {
                revert AppStorage.InvalidRequest(
                    "ChainSecured adminWalletAddress must equal the sender."
                );
            }
        }
        if (s.allApiKeyHashesToMaster[apiKeyHash] != 0) {
            revert AppStorage.AccountAlreadyExists(apiKeyHash);
        }
        AppStorage.Account storage account = s.accounts[apiKeyHash];
        account.managed = managed;
        account.adminWalletAddress = adminWalletAddress;
        account.billingWalletAddress = adminWalletAddress;
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

    /// @notice Convert an existing managed (API-mode) account into a ChainSecured (sovereign)
    ///         account by reassigning its admin wallet to a user-controlled address.
    /// @dev    Only callable by an api_payer (or diamond owner) since a managed account has
    ///         no on-chain admin yet. The conversion is one-way: re-running on an already
    ///         unmanaged account reverts. The apiKeyHash is preserved so existing groups,
    ///         actions, PKPs, and usage keys remain attached to the same account.
    function convertToChainSecuredAccount(
        uint256 apiKeyHash,
        address newAdminWalletAddress
    ) public {
        SecurityLib.revertIfNotApiPayerOrOwner(msg.sender);
        if (newAdminWalletAddress == address(0)) {
            revert AppStorage.InvalidRequest(
                "newAdminWalletAddress must be non-zero"
            );
        }
        uint256 newApiKeyHash = uint256(
            keccak256(abi.encodePacked(newAdminWalletAddress))
        );
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        if (s.allApiKeyHashesToMaster[newApiKeyHash] != 0) {
            revert AppStorage.AccountAlreadyExists(newApiKeyHash);
        }
        if (s.allApiKeyHashesToMaster[apiKeyHash] != apiKeyHash) {
            revert AppStorage.AccountDoesNotExist(apiKeyHash);
        }
        AppStorage.Account storage account = s.accounts[apiKeyHash];
        if (!account.managed) {
            revert AppStorage.InvalidRequest(
                "Account is already ChainSecured."
            );
        }
        s.allApiKeyHashesToMaster[newApiKeyHash] = apiKeyHash; // effectively map the new admin wallet to the existing account, without removing the old one.
        account.managed = false;
        if (account.billingWalletAddress == address(0)) {
            account.billingWalletAddress = account.adminWalletAddress;
        } // otherwise, keep the existing billing wallet address
        account.adminWalletAddress = newAdminWalletAddress;
        emit AccountConvertedToChainSecured(apiKeyHash, newAdminWalletAddress);
    }

    /// @notice Transfer ownership of a ChainSecured (unmanaged) account from the
    ///         current admin wallet to a new wallet. Only the current admin may
    ///         call this; the api_payer has no authority over ChainSecured
    ///         accounts. The master apiKeyHash and billing wallet are preserved
    ///         so groups, actions, PKPs, usage keys, and billing remain
    ///         attached.
    /// @dev    Accepts either the master apiKeyHash or any hash that resolves
    ///         to it (e.g. keccak256(currentAdminWalletAddress) for accounts
    ///         that have already been transferred once). The previous admin's
    ///         `allApiKeyHashesToMaster` entry is left in place intentionally
    ///         — for an account originally created via `newChainSecuredAccount`
    ///         it equals the master hash itself, so removing it would orphan
    ///         the account storage. A side-effect is that ownership can't be
    ///         transferred back to a wallet that has ever been admin of any
    ///         account, even after a forward transfer.
    function transferChainSecuredAccountOwnership(
        uint256 apiKeyHash,
        address newAdminWalletAddress
    ) public {
        if (newAdminWalletAddress == address(0)) {
            revert AppStorage.InvalidRequest(
                "newAdminWalletAddress must be non-zero"
            );
        }
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        uint256 masterApiKeyHash = s.allApiKeyHashesToMaster[apiKeyHash];
        if (masterApiKeyHash == 0) {
            revert AppStorage.AccountDoesNotExist(apiKeyHash);
        }
        AppStorage.Account storage account = s.accounts[masterApiKeyHash];
        if (account.managed) {
            revert AppStorage.InvalidRequest(
                "Account is not ChainSecured; use convertToChainSecuredAccount instead."
            );
        }
        if (msg.sender != account.adminWalletAddress) {
            revert AppStorage.NoAccountAccess(apiKeyHash, msg.sender);
        }
        if (newAdminWalletAddress == account.adminWalletAddress) {
            revert AppStorage.InvalidRequest(
                "newAdminWalletAddress must differ from current admin"
            );
        }
        uint256 newApiKeyHash = uint256(
            keccak256(abi.encodePacked(newAdminWalletAddress))
        );
        if (s.allApiKeyHashesToMaster[newApiKeyHash] != 0) {
            revert AppStorage.AccountAlreadyExists(newApiKeyHash);
        }
        s.allApiKeyHashesToMaster[newApiKeyHash] = masterApiKeyHash;
        address previousAdminWalletAddress = account.adminWalletAddress;
        account.adminWalletAddress = newAdminWalletAddress;
        emit ChainSecuredAccountOwnershipTransferred(
            masterApiKeyHash,
            previousAdminWalletAddress,
            newAdminWalletAddress
        );
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
        // Zero is the "does not exist" sentinel for allApiKeyHashesToMaster and
        // for usageApiKeys[h].apiKeyHash. Allowing a zero usage hash would let an
        // account claim allApiKeyHashesToMaster[0] and corrupt every existence
        // check that treats 0 as "unset".
        if (usageApiKeyHash == 0) {
            revert AppStorage.InvalidRequest("usageApiKeyHash must be non-zero");
        }
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
        // Guard the account-resolution entry. Without this an attacker holding
        // any account could call setUsageApiKey(theirHash, victimMasterHash, ...)
        // and overwrite allApiKeyHashesToMaster[victimMasterHash], hijacking
        // every later resolution of the victim's hash.
        uint256 existingUsageMaster = s.allApiKeyHashesToMaster[usageApiKeyHash];
        if (existingUsageMaster != 0) {
            // The hash already resolves to an account. Only let it through when
            // it is already a usage key OF THIS account (a legitimate update).
            // Everything else is rejected:
            //   - a master hash (resolves to itself),
            //   - a hash registered to a different account (cross-account hijack),
            //   - this account's own wallet/resolver alias created by
            //     convertToChainSecuredAccount / transferChainSecuredAccountOwnership
            //     that is not a usage key — promoting it would let a later
            //     removeUsageApiKey delete the alias and orphan the wallet.
            bool isExistingUsageKeyForThisAccount = existingUsageMaster ==
                masterAccountApiKeyHash &&
                s
                .accounts[masterAccountApiKeyHash]
                .usageApiKeys[usageApiKeyHash].apiKeyHash ==
                usageApiKeyHash;
            if (!isExistingUsageKeyForThisAccount) {
                revert AppStorage.AccountAlreadyExists(usageApiKeyHash);
            }
        }
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
        // Global first-owner binding. Derivation paths are public on-chain and
        // the key is a stateless function of the path, so without this check a
        // different account could register an already-registered pkpId under
        // its own account and drive the node to sign with the victim's key.
        // The first master account to register a pkpId owns it forever; the
        // binding survives removeWalletDerivation, so only the original owner
        // may ever re-register a deleted address (recovery flow stays intact).
        uint256 existingOwner = s.pkpIdToOwnerMaster[pkpId];
        if (existingOwner == 0) {
            s.pkpIdToOwnerMaster[pkpId] = masterHash;
        } else if (existingOwner != masterHash) {
            revert AppStorage.InvalidRequest("PKP owned by another account");
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

    /// @notice Permanently and irreversibly remove a registered wallet (PKP) from an account.
    /// @dev HARD DELETE. This wipes the on-chain metadata for the wallet, including its
    ///      `derivationPath`. Keys are stateless derivations from that path and are never
    ///      stored anywhere else, so once the path is deleted the private key can never be
    ///      re-derived. Anything encrypted or otherwise secured by this wallet becomes
    ///      permanently unrecoverable. There is no undo. The wallet is also removed from
    ///      every group it belongs to. Only the master account may call this.
    ///
    ///      The global `allPkpIds` "ever generated" ledger is intentionally left untouched;
    ///      it records only the address (never the path) and serves as an append-only history.
    ///
    ///      The global `pkpIdToOwnerMaster` binding is also intentionally kept: even after a
    ///      hard delete, only the original master account may ever re-register this address.
    ///      This closes the delete/re-register race that would otherwise let another account
    ///      claim the address (and thus the key, which is a stateless function of the path).
    function removeWalletDerivation(uint256 apiKeyHash, address pkpId) public {
        SecurityLib.revertIfNoAccountAccess(apiKeyHash, msg.sender);
        SecurityLib.revertIfNotMasterAccount(apiKeyHash);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        AppStorage.Account storage account = s.accounts[apiKeyHash];
        // pkpData[pkpId].id is the derivationPath, which registerWalletDerivation
        // guarantees is non-zero for a registered wallet. Zero => not registered.
        if (account.pkpData[pkpId].id == 0) {
            revert AppStorage.InvalidRequest("PKP not registered");
        }

        // Swap-and-pop within the counter-indexed pkpIds mapping so that listPkps
        // (which iterates indices 0..pkpCount) stays gap-free after removal.
        uint256 count = account.pkpCount;
        for (uint256 i = 0; i < count; i++) {
            if (account.pkpIds[i] == pkpId) {
                if (i != count - 1) {
                    account.pkpIds[i] = account.pkpIds[count - 1];
                }
                delete account.pkpIds[count - 1];
                account.pkpCount = count - 1;
                break;
            }
        }

        // Wipe the wallet metadata. Deleting derivationPath here is what makes the
        // key permanently unrecoverable.
        delete account.pkpData[pkpId];

        // Remove the wallet from every group that references it to avoid stale entries.
        uint256 groupCount = account.groupList.length();
        for (uint256 i = 0; i < groupCount; i++) {
            uint256 groupId = account.groupList.at(i);
            account.groups[groupId].pkpId.remove(pkpId);
        }

        emit WalletDerivationRemoved(apiKeyHash, pkpId);
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
