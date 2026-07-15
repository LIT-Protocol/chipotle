// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {BaseTest} from "./helpers/BaseTest.sol";
import {AppStorage} from "../contracts/AccountConfigFacets/AppStorage.sol";
import {ViewsFacet} from "../contracts/AccountConfigFacets/ViewsFacet.sol";

contract AccountsTest is BaseTest {
    function test_newChainSecuredAccount_writesPersistAndAreReadable() public {
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");

        uint256 hash = apiKeyHashOf(user);
        assertEq(views_.getAccountWalletAddress(hash), user);
        assertEq(views_.getBillingWalletAddress(hash), user);
        assertEq(views_.accountCount(), 1);
        assertEq(views_.indexToAccountHashAt(1), hash);
    }

    function test_newChainSecuredAccount_rejectsDuplicate() public {
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");

        uint256 hash = apiKeyHashOf(user);
        vm.prank(user);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.AccountAlreadyExists.selector, hash));
        writes.newChainSecuredAccount("alice2", "second");
    }

    function test_newAccount_apiPayerCanCreateManagedAccount() public {
        uint256 hash = uint256(keccak256("api-key-1"));
        vm.prank(apiPayer);
        writes.newAccount(hash, true, "managed", "by api payer", user);

        assertEq(views_.getAccountWalletAddress(hash), user);
    }

    function test_newAccount_strangerCannotCreateManagedAccount() public {
        // Non-api-payer can only create unmanaged accounts whose apiKeyHash matches their wallet.
        uint256 hash = uint256(keccak256("api-key-1"));
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "ChainSecured accounts must be unmanaged."
            )
        );
        writes.newAccount(hash, true, "managed", "no go", stranger);
    }

    function test_newAccount_strangerHashMismatchReverts() public {
        // Even unmanaged, the apiKeyHash for a non-api-payer caller must equal keccak(sender).
        uint256 hash = uint256(keccak256("not-my-hash"));
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "ChainSecured apiKeyHash must equal the keccak256 of the sender."
            )
        );
        writes.newAccount(hash, false, "x", "x", stranger);
    }

    function test_addGroup_thenListGroups() public {
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");
        uint256 hash = apiKeyHashOf(user);

        uint256[] memory cidHashes = new uint256[](2);
        cidHashes[0] = uint256(keccak256("cid-a"));
        cidHashes[1] = uint256(keccak256("cid-b"));
        address[] memory pkpIds = new address[](0);

        vm.prank(user);
        uint256 groupId = writes.addGroup(hash, "g1", "first group", cidHashes, pkpIds);
        assertEq(groupId, 1);

        AppStorage.Metadata[] memory groups = views_.listGroups(hash, 0, 10);
        assertEq(groups.length, 1);
        assertEq(groups[0].id, 1);
        assertEq(groups[0].name, "g1");

        ViewsFacet.GroupReturn memory group = views_.listGroupContents(hash, groupId);
        assertEq(group.cidHash.length, 2);
    }

    function test_setUsageApiKey_thenListApiKeys() public {
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");
        uint256 hash = apiKeyHashOf(user);

        uint256 usageHash = uint256(keccak256("usage-key-1"));
        uint256[] memory empty = new uint256[](0);
        vm.prank(user);
        writes.setUsageApiKey(
            hash,
            usageHash,
            block.timestamp + 7 days,
            0,
            "usage-1",
            "limited usage key",
            false,
            false,
            false,
            empty,
            empty,
            empty,
            empty
        );

        ViewsFacet.UsageApiKeyReturn[] memory keys = views_.listApiKeys(hash, 0, 10);
        assertEq(keys.length, 1);
        assertEq(keys[0].apiKeyHash, usageHash);
        assertEq(keys[0].metadata.name, "usage-1");

        // The usage key should resolve to the same master account.
        assertEq(views_.getAccountWalletAddress(usageHash), user);
    }

    function test_setUsageApiKey_rejectsHijackOfAnotherAccount() public {
        // Victim creates a ChainSecured account.
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");
        uint256 victimMaster = apiKeyHashOf(user);

        // Attacker creates their own ChainSecured account, passing the access
        // check on their own hash.
        vm.prank(stranger);
        writes.newChainSecuredAccount("mallory", "evil");

        // Attacker tries to point the victim's master hash at their own account
        // by setting it as one of their usage keys. This must revert instead of
        // overwriting allApiKeyHashesToMaster[victimMaster].
        uint256[] memory empty = new uint256[](0);
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.AccountAlreadyExists.selector,
                victimMaster
            )
        );
        writes.setUsageApiKey(
            apiKeyHashOf(stranger),
            victimMaster,
            block.timestamp + 7 days,
            0,
            "hijack",
            "hijack",
            false,
            false,
            false,
            empty,
            empty,
            empty,
            empty
        );

        // Victim's hash still resolves to the victim.
        assertEq(views_.getAccountWalletAddress(victimMaster), user);
    }

    function test_setUsageApiKey_rejectsUsageKeyOwnedByAnotherAccount() public {
        // Victim creates an account and registers a usage key.
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");
        uint256 victimMaster = apiKeyHashOf(user);

        uint256 usageHash = uint256(keccak256("shared-usage-key"));
        uint256[] memory empty = new uint256[](0);
        vm.prank(user);
        writes.setUsageApiKey(
            victimMaster,
            usageHash,
            block.timestamp + 7 days,
            0,
            "usage-1",
            "victim usage key",
            false,
            false,
            false,
            empty,
            empty,
            empty,
            empty
        );

        // Attacker creates their own account and tries to claim the victim's
        // usage-key hash as one of their own usage keys.
        vm.prank(stranger);
        writes.newChainSecuredAccount("mallory", "evil");

        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.AccountAlreadyExists.selector,
                usageHash
            )
        );
        writes.setUsageApiKey(
            apiKeyHashOf(stranger),
            usageHash,
            block.timestamp + 7 days,
            0,
            "steal",
            "steal",
            false,
            false,
            false,
            empty,
            empty,
            empty,
            empty
        );

        // The usage key still resolves to the victim.
        assertEq(views_.getAccountWalletAddress(usageHash), user);
    }

    function test_setUsageApiKey_allowsUpdatingOwnUsageKey() public {
        // Re-calling setUsageApiKey for an existing usage key of the same
        // account must still succeed (the guard only blocks cross-account
        // collisions).
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");
        uint256 hash = apiKeyHashOf(user);

        uint256 usageHash = uint256(keccak256("usage-key-1"));
        uint256[] memory empty = new uint256[](0);
        vm.prank(user);
        writes.setUsageApiKey(
            hash,
            usageHash,
            block.timestamp + 7 days,
            0,
            "usage-1",
            "first",
            false,
            false,
            false,
            empty,
            empty,
            empty,
            empty
        );

        // Update it again with new metadata — should not revert.
        vm.prank(user);
        writes.setUsageApiKey(
            hash,
            usageHash,
            block.timestamp + 14 days,
            0,
            "usage-1-updated",
            "second",
            true,
            false,
            false,
            empty,
            empty,
            empty,
            empty
        );

        ViewsFacet.UsageApiKeyReturn[] memory keys = views_.listApiKeys(hash, 0, 10);
        assertEq(keys.length, 1);
        assertEq(keys[0].metadata.name, "usage-1-updated");
    }

    function test_addAction_thenListActions_thenRemove() public {
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");
        uint256 hash = apiKeyHashOf(user);

        uint256 actionHash = uint256(keccak256("action-cid"));
        vm.prank(user);
        writes.addAction(hash, "act1", "first action", actionHash);

        AppStorage.Metadata[] memory actions = views_.listActions(hash, 0, 10);
        assertEq(actions.length, 1);
        assertEq(actions[0].id, actionHash);
        assertEq(actions[0].name, "act1");

        vm.prank(user);
        writes.removeAction(hash, actionHash);

        actions = views_.listActions(hash, 0, 10);
        assertEq(actions.length, 0);
    }

    function test_registerWalletDerivation_storesAndIsListable() public {
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");
        uint256 hash = apiKeyHashOf(user);

        address pkpAddr = address(0xBEEF);
        vm.prank(user);
        writes.registerWalletDerivation(hash, pkpAddr, 42, "pkp-1", "first pkp");

        assertEq(views_.getWalletDerivation(hash, pkpAddr), 42);
        AppStorage.PkpData[] memory pkps = views_.listPkps(hash, 0, 10);
        assertEq(pkps.length, 1);
        assertEq(pkps[0].pkpId, pkpAddr);

        // Re-registering same pkp must revert.
        vm.prank(user);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.InvalidRequest.selector, "PKP already registered")
        );
        writes.registerWalletDerivation(hash, pkpAddr, 43, "dup", "dup");
    }

    function test_removeWalletDerivation_hardDeletesAndCompacts() public {
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");
        uint256 hash = apiKeyHashOf(user);

        address pkpA = address(0xA11CE);
        address pkpB = address(0xB0B);
        address pkpC = address(0xCA75);
        vm.startPrank(user);
        writes.registerWalletDerivation(hash, pkpA, 11, "a", "a");
        writes.registerWalletDerivation(hash, pkpB, 22, "b", "b");
        writes.registerWalletDerivation(hash, pkpC, 33, "c", "c");
        vm.stopPrank();

        // Remove the middle wallet.
        vm.prank(user);
        writes.removeWalletDerivation(hash, pkpB);

        // Derivation path is wiped (hard delete → unrecoverable).
        assertEq(views_.getWalletDerivation(hash, pkpB), 0);

        // listPkps stays gap-free with the survivors (swap-and-pop compaction).
        AppStorage.PkpData[] memory pkps = views_.listPkps(hash, 0, 10);
        assertEq(pkps.length, 2);
        bool sawA;
        bool sawC;
        for (uint256 i = 0; i < pkps.length; i++) {
            if (pkps[i].pkpId == pkpA) sawA = true;
            if (pkps[i].pkpId == pkpC) sawC = true;
            assertTrue(pkps[i].pkpId != pkpB, "deleted pkp still listed");
        }
        assertTrue(sawA && sawC, "survivors missing after compaction");

        // The address can be registered again after deletion (metadata fully cleared).
        vm.prank(user);
        writes.registerWalletDerivation(hash, pkpB, 44, "b2", "b2");
        assertEq(views_.getWalletDerivation(hash, pkpB), 44);
    }

    function test_registerWalletDerivation_crossAccountHijackReverts() public {
        // Victim registers a wallet; its derivationPath is public on-chain.
        vm.prank(user);
        writes.newChainSecuredAccount("victim", "victim");
        uint256 victimHash = apiKeyHashOf(user);

        address pkpAddr = address(0xBEEF);
        vm.prank(user);
        writes.registerWalletDerivation(victimHash, pkpAddr, 42, "v", "v");
        assertEq(views_.getPkpOwnerMaster(pkpAddr), victimHash);

        // Attacker with their own account cannot register the victim's pkpId,
        // even though the attacker's account has no entry for it.
        vm.prank(stranger);
        writes.newChainSecuredAccount("attacker", "attacker");
        uint256 attackerHash = apiKeyHashOf(stranger);

        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "PKP owned by another account"
            )
        );
        writes.registerWalletDerivation(attackerHash, pkpAddr, 42, "a", "a");
    }

    function test_pkpOwnerBinding_survivesRemoveWalletDerivation() public {
        vm.prank(user);
        writes.newChainSecuredAccount("victim", "victim");
        uint256 victimHash = apiKeyHashOf(user);
        vm.prank(stranger);
        writes.newChainSecuredAccount("attacker", "attacker");
        uint256 attackerHash = apiKeyHashOf(stranger);

        address pkpAddr = address(0xBEEF);
        vm.prank(user);
        writes.registerWalletDerivation(victimHash, pkpAddr, 42, "v", "v");
        vm.prank(user);
        writes.removeWalletDerivation(victimHash, pkpAddr);

        // Binding is kept after the hard delete.
        assertEq(views_.getPkpOwnerMaster(pkpAddr), victimHash);

        // Attacker still cannot claim the deleted address.
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "PKP owned by another account"
            )
        );
        writes.registerWalletDerivation(attackerHash, pkpAddr, 42, "a", "a");

        // The original owner can re-register (recovery / re-add after delete).
        vm.prank(user);
        writes.registerWalletDerivation(victimHash, pkpAddr, 42, "v2", "v2");
        assertEq(views_.getWalletDerivation(victimHash, pkpAddr), 42);
        assertEq(views_.getPkpOwnerMaster(pkpAddr), victimHash);
    }

    function test_backfillPkpOwners_bindsLegacyWalletsAndBlocksHijack() public {
        vm.prank(user);
        writes.newChainSecuredAccount("legacy", "legacy");
        uint256 legacyHash = apiKeyHashOf(user);
        vm.prank(stranger);
        writes.newChainSecuredAccount("attacker", "attacker");
        uint256 attackerHash = apiKeyHashOf(stranger);

        // Simulate a pre-migration wallet: registered in the account but with no
        // global owner binding (as if registered before the upgrade).
        address legacyPkp = address(0x1E9AC7);
        assertEq(views_.getPkpOwnerMaster(legacyPkp), 0);

        address[] memory pkpIds = new address[](1);
        pkpIds[0] = legacyPkp;
        uint256[] memory masters = new uint256[](1);
        masters[0] = legacyHash;

        // Only the diamond owner or config operator may backfill.
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.OnlyConfigOperatorOrOwner.selector,
                stranger
            )
        );
        writes.backfillPkpOwners(pkpIds, masters);

        vm.prank(owner);
        writes.backfillPkpOwners(pkpIds, masters);
        assertEq(views_.getPkpOwnerMaster(legacyPkp), legacyHash);

        // Backfill never re-assigns an existing binding (idempotent, skip-if-set).
        masters[0] = attackerHash;
        vm.prank(owner);
        writes.backfillPkpOwners(pkpIds, masters);
        assertEq(views_.getPkpOwnerMaster(legacyPkp), legacyHash);

        // Post-backfill, the attacker cannot register the legacy wallet...
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "PKP owned by another account"
            )
        );
        writes.registerWalletDerivation(attackerHash, legacyPkp, 7, "a", "a");

        // ...but the legacy owner can (e.g. #450 recovery re-registration).
        vm.prank(user);
        writes.registerWalletDerivation(legacyHash, legacyPkp, 7, "l", "l");
        assertEq(views_.getWalletDerivation(legacyHash, legacyPkp), 7);
    }

    function test_removeWalletDerivation_unregisteredReverts() public {
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");
        uint256 hash = apiKeyHashOf(user);

        vm.prank(user);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.InvalidRequest.selector, "PKP not registered")
        );
        writes.removeWalletDerivation(hash, address(0xDEAD));
    }

    function test_views_unknownAccountReverts() public {
        // Any read that resolves through `getReadOnlyAccount` should revert for a
        // hash that has never been written. Using `getAccountWalletAddress` as the
        // probe — the same revert path covers every account-scoped view.
        uint256 hash = apiKeyHashOf(user);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.AccountDoesNotExist.selector, hash));
        views_.getAccountWalletAddress(hash);
    }

    function test_convertToChainSecured_apiPayerOnly() public {
        // First create a managed account via api payer.
        uint256 hash = uint256(keccak256("managed-key"));
        vm.prank(apiPayer);
        writes.newAccount(hash, true, "managed", "via api payer", apiPayer);

        // Stranger cannot convert.
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.OnlyApiPayerOrOwner.selector, stranger)
        );
        writes.convertToChainSecuredAccount(hash, user);

        // Api payer can; afterward the new admin wallet hash also resolves.
        vm.prank(apiPayer);
        writes.convertToChainSecuredAccount(hash, user);
        assertEq(views_.getAccountWalletAddress(hash), user);
        assertEq(views_.getAccountWalletAddress(apiKeyHashOf(user)), user);
    }

    function test_convertToChainSecured_secondCallReverts() public {
        // Already-unmanaged accounts can't be re-converted.
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");
        uint256 hash = apiKeyHashOf(user);

        vm.prank(apiPayer);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "Account is already ChainSecured."
            )
        );
        writes.convertToChainSecuredAccount(hash, stranger);
    }

    function test_setUsageApiKey_rejectsZeroUsageHash() public {
        // Zero is the "does not exist" sentinel. Registering a zero usage hash
        // would claim allApiKeyHashesToMaster[0] and corrupt existence checks.
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");
        uint256 hash = apiKeyHashOf(user);

        uint256[] memory empty = new uint256[](0);
        vm.prank(user);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "usageApiKeyHash must be non-zero"
            )
        );
        writes.setUsageApiKey(
            hash,
            0,
            block.timestamp + 7 days,
            0,
            "zero",
            "zero",
            false,
            false,
            false,
            empty,
            empty,
            empty,
            empty
        );

        // Nothing was registered; the zero slot must stay unclaimed.
        ViewsFacet.UsageApiKeyReturn[] memory keys = views_.listApiKeys(hash, 0, 10);
        assertEq(keys.length, 0);
    }

    function test_setUsageApiKey_rejectsPromotingWalletAlias() public {
        // convertToChainSecuredAccount maps the new admin wallet hash to the
        // account as a resolver alias, but that hash is NOT a usage key.
        // setUsageApiKey must refuse to turn the alias into a usage key —
        // otherwise a later removeUsageApiKey would delete the alias from
        // allApiKeyHashesToMaster and orphan the admin wallet.
        uint256 master = uint256(keccak256("managed-key"));
        vm.prank(apiPayer);
        writes.newAccount(master, true, "managed", "via api payer", apiPayer);
        vm.prank(apiPayer);
        writes.convertToChainSecuredAccount(master, user);

        uint256 aliasHash = apiKeyHashOf(user);
        // Sanity: the alias resolves to the account but is not a usage key.
        assertEq(views_.getAccountWalletAddress(aliasHash), user);

        uint256[] memory empty = new uint256[](0);
        vm.prank(user);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.AccountAlreadyExists.selector,
                aliasHash
            )
        );
        writes.setUsageApiKey(
            master,
            aliasHash,
            block.timestamp + 7 days,
            0,
            "promote",
            "promote",
            false,
            false,
            false,
            empty,
            empty,
            empty,
            empty
        );

        // The alias is untouched and still resolves to the account.
        assertEq(views_.getAccountWalletAddress(aliasHash), user);
        ViewsFacet.UsageApiKeyReturn[] memory keys = views_.listApiKeys(master, 0, 10);
        assertEq(keys.length, 0);
    }
}
