// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {BaseTest} from "./helpers/BaseTest.sol";
import {AppStorage} from "../contracts/AccountConfigFacets/AppStorage.sol";
import {ViewsFacet} from "../contracts/AccountConfigFacets/ViewsFacet.sol";
import {IDiamond} from "../interfaces/IDiamond.sol";
import {FunctionNotFound} from "../contracts/AccountConfig.sol";
import {SecurityLib} from "../contracts/AccountConfigFacets/SecurityLib.sol";

// Historical migration facet, used only to exercise its removal from a diamond.
contract LegacyPathBackfill {
    event PathOwnerBackfilled(
        uint256 indexed derivationPath,
        uint256 indexed masterHash
    );

    /// @notice One-time migration helper: bind derivation paths registered before
    ///         the global path-owner binding existed to their original master
    ///         account. Companion to backfillPkpOwners — until a path is
    ///         backfilled, getWalletDerivation falls through (pathOwner == 0), so
    ///         the aliasing hole stays open for that path. Run this over every
    ///         historical path to fully close it for pre-fix wallets.
    /// @dev Pairs should be derived off-chain from the EARLIEST
    ///      `WalletDerivationRegistered(masterHash, pkpId, derivationPath)` event
    ///      per derivationPath (first registration wins, matching the rule
    ///      registerWalletDerivation now enforces). Already-bound paths are
    ///      skipped, never re-assigned, so the call is idempotent and safe to
    ///      re-run. Restricted to the diamond owner or config operator.
    function backfillPathOwners(
        uint256[] calldata derivationPaths,
        uint256[] calldata masterHashes
    ) public {
        SecurityLib.revertIfNotConfigOperatorOrOwner(msg.sender);
        if (derivationPaths.length != masterHashes.length) {
            revert AppStorage.InvalidRequest("array length mismatch");
        }
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        for (uint256 i = 0; i < derivationPaths.length; i++) {
            if (masterHashes[i] == 0) {
                revert AppStorage.InvalidRequest("masterHash must be non-zero");
            }
            if (derivationPaths[i] == 0) {
                revert AppStorage.InvalidRequest("derivationPath must be non-zero");
            }
            if (s.pathToOwnerMaster[derivationPaths[i]] != 0) {
                continue; // already bound — never re-assign ownership
            }
            s.pathToOwnerMaster[derivationPaths[i]] = masterHashes[i];
            emit PathOwnerBackfilled(derivationPaths[i], masterHashes[i]);
        }
    }
}

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

    /// @dev Storage slot of `pkpIdToOwnerMaster[pkpId]`. The mapping is at field
    ///      index 18 (counting the two EnumerableSet fields as 2 slots each) at
    ///      base slot keccak256("com.litprotocol.accountconfig.storage"). Used to
    ///      plant the pre-fix on-chain state that the public API can no longer
    ///      create.
    function _pkpOwnerSlot(address pkpId) internal pure returns (bytes32) {
        bytes32 base = keccak256("com.litprotocol.accountconfig.storage");
        bytes32 mapSlot = bytes32(uint256(base) + 18);
        return keccak256(abi.encode(pkpId, mapSlot));
    }

    /// @dev Storage slot of `pathToOwnerMaster[derivationPath]`. Appended
    ///      immediately after pkpIdToOwnerMaster, so field index 19.
    function _pathOwnerSlot(
        uint256 derivationPath
    ) internal pure returns (bytes32) {
        bytes32 base = keccak256("com.litprotocol.accountconfig.storage");
        bytes32 mapSlot = bytes32(uint256(base) + 19);
        return keccak256(abi.encode(derivationPath, mapSlot));
    }

    /// The core path-aliasing attack: the private key is a stateless function of
    /// the derivationPath, and paths are public. The #575 pkpId binding only
    /// protects the address label, so an attacker could register a FRESH,
    /// self-owned pkpId carrying the victim's public path and drive the node to
    /// release the victim's key. The path first-owner binding blocks that at
    /// registration — note the attacker uses a DIFFERENT pkpId than the victim,
    /// so it is the path binding, not the pkpId binding, doing the work.
    function test_registerWalletDerivation_pathAliasingAcrossAccountsReverts()
        public
    {
        uint256 victimPath = 42;

        vm.prank(user);
        writes.newChainSecuredAccount("victim", "victim");
        uint256 victimHash = apiKeyHashOf(user);
        address victimPkp = address(0xBEEF);
        vm.prank(user);
        writes.registerWalletDerivation(
            victimHash,
            victimPkp,
            victimPath,
            "v",
            "v"
        );
        assertEq(views_.getPathOwnerMaster(victimPath), victimHash);

        vm.prank(stranger);
        writes.newChainSecuredAccount("attacker", "attacker");
        uint256 attackerHash = apiKeyHashOf(stranger);

        // Fresh pkpId label the attacker legitimately owns, aliased to the
        // victim's path. The pkpId binding would let this through (new address);
        // the path binding is what must revert.
        address attackerPkp = address(0xF00D);
        assertEq(views_.getPkpOwnerMaster(attackerPkp), 0);

        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "derivation path owned by another account"
            )
        );
        writes.registerWalletDerivation(
            attackerHash,
            attackerPkp,
            victimPath,
            "a",
            "a"
        );
    }

    function test_pathOwnerBinding_survivesRemoveAndAllowsOwnerReRegister()
        public
    {
        uint256 path = 42;

        vm.prank(user);
        writes.newChainSecuredAccount("victim", "victim");
        uint256 victimHash = apiKeyHashOf(user);
        vm.prank(stranger);
        writes.newChainSecuredAccount("attacker", "attacker");
        uint256 attackerHash = apiKeyHashOf(stranger);

        address victimPkp = address(0xBEEF);
        vm.prank(user);
        writes.registerWalletDerivation(victimHash, victimPkp, path, "v", "v");

        // Hard delete wipes the account-local entry, but the path binding is
        // kept (symmetric with the pkpId binding) so the key stays claimable
        // only by its first owner.
        vm.prank(user);
        writes.removeWalletDerivation(victimHash, victimPkp);
        assertEq(views_.getPathOwnerMaster(path), victimHash);

        // Attacker cannot claim the freed path under any fresh label.
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "derivation path owned by another account"
            )
        );
        writes.registerWalletDerivation(
            attackerHash,
            address(0xF00D),
            path,
            "a",
            "a"
        );

        // The original owner can re-register the path (recovery flow intact).
        vm.prank(user);
        writes.registerWalletDerivation(victimHash, victimPkp, path, "v2", "v2");
        assertEq(views_.getWalletDerivation(victimHash, victimPkp), path);
    }

    function test_getWalletDerivation_preExistingPathAliasFailsClosed() public {
        uint256 path = 42;

        // Attacker holds a legitimate account-local entry keyed on a fresh label
        // pointing at `path` — exactly the stale row a pre-fix aliasing attack
        // would have left behind (pkpId owner = attacker, so the pkpId check
        // passes).
        vm.prank(stranger);
        writes.newChainSecuredAccount("attacker", "attacker");
        uint256 attackerHash = apiKeyHashOf(stranger);
        address attackerPkp = address(0xF00D);
        vm.prank(stranger);
        writes.registerWalletDerivation(
            attackerHash,
            attackerPkp,
            path,
            "a",
            "a"
        );

        // Sanity-check the slot math against the contract's own getter before
        // relying on vm.store (guards against storage-layout drift).
        assertEq(
            uint256(vm.load(address(views_), _pathOwnerSlot(path))),
            attackerHash
        );

        // Simulate post-backfill truth: the VICTIM was the real first owner of
        // the path. (The attacker still owns the pkpId label locally.)
        uint256 victimHash = apiKeyHashOf(user);
        vm.store(address(views_), _pathOwnerSlot(path), bytes32(victimHash));
        assertEq(views_.getPathOwnerMaster(path), victimHash);

        // The node's key-release path resolves via getWalletDerivation with the
        // caller's account hash. The pkpId check passes (attacker owns the
        // label), but the path check now fails closed — neutralizing an alias
        // that predates the upgrade.
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "derivation path owned by another account"
            )
        );
        views_.getWalletDerivation(attackerHash, attackerPkp);
    }

    function test_removeBackfillPathOwners_preservesBindingsAndBlocksAliasing()
        public
    {
        bytes4 selector = LegacyPathBackfill.backfillPathOwners.selector;
        // Fresh deployments must not expose the retired selector.
        assertEq(loupe.facetAddress(selector), address(0));
        bytes4[] memory selectors = new bytes4[](1);
        selectors[0] = selector;
        IDiamond.FacetCut[] memory cuts = new IDiamond.FacetCut[](1);
        cuts[0] = IDiamond.FacetCut(
            address(new LegacyPathBackfill()), IDiamond.FacetCutAction.Add, selectors
        );
        vm.prank(owner);
        cut.diamondCut(cuts, address(0), "");
        LegacyPathBackfill legacy = LegacyPathBackfill(d.diamond);
        uint256 legacyPath = 4242;

        vm.prank(user);
        writes.newChainSecuredAccount("legacy", "legacy");
        uint256 legacyHash = apiKeyHashOf(user);
        vm.prank(stranger);
        writes.newChainSecuredAccount("attacker", "attacker");
        uint256 attackerHash = apiKeyHashOf(stranger);

        // Recreate a pre-fix alias: the label belongs to the attacker, but
        // the historical path has not yet been bound to its original owner.
        vm.prank(stranger);
        writes.registerWalletDerivation(attackerHash, address(0xCAFE), legacyPath, "a", "a");
        assertEq(uint256(vm.load(address(views_), _pathOwnerSlot(legacyPath))), attackerHash);
        vm.store(address(views_), _pathOwnerSlot(legacyPath), bytes32(0));
        assertEq(views_.getPathOwnerMaster(legacyPath), 0);

        uint256[] memory paths = new uint256[](1);
        paths[0] = legacyPath;
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
        legacy.backfillPathOwners(paths, masters);

        vm.prank(owner);
        legacy.backfillPathOwners(paths, masters);
        assertEq(views_.getPathOwnerMaster(legacyPath), legacyHash);

        // Idempotent: never re-assigns an existing binding.
        masters[0] = attackerHash;
        vm.prank(owner);
        legacy.backfillPathOwners(paths, masters);
        assertEq(views_.getPathOwnerMaster(legacyPath), legacyHash);

        vm.prank(user);
        writes.registerWalletDerivation(legacyHash, address(0xBEEF), legacyPath, "l", "l");

        // Retire the selector using the same Remove action as the deployment manifest.
        cuts[0] = IDiamond.FacetCut(address(0), IDiamond.FacetCutAction.Remove, selectors);
        vm.prank(owner);
        cut.diamondCut(cuts, address(0), "");
        assertEq(loupe.facetAddress(selector), address(0));
        vm.prank(owner);
        vm.expectRevert(abi.encodeWithSelector(FunctionNotFound.selector, selector));
        legacy.backfillPathOwners(paths, masters);
        assertEq(views_.getPathOwnerMaster(legacyPath), legacyHash);

        assertEq(views_.getWalletDerivation(legacyHash, address(0xBEEF)), legacyPath);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "derivation path owned by another account"
            )
        );
        views_.getWalletDerivation(attackerHash, address(0xCAFE));

        // Post-backfill, the attacker cannot alias the legacy path...
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "derivation path owned by another account"
            )
        );
        writes.registerWalletDerivation(
            attackerHash,
            address(0xF00D),
            legacyPath,
            "a",
            "a"
        );

        // ...but the legacy owner can still register another label for it.
        vm.prank(user);
        writes.registerWalletDerivation(
            legacyHash,
            address(0xBEF0),
            legacyPath,
            "l",
            "l"
        );
        assertEq(
            views_.getWalletDerivation(legacyHash, address(0xBEF0)),
            legacyPath
        );
    }

    function test_getWalletDerivation_preExistingHijackFailsClosed() public {
        // Register under the attacker legitimately: sets owner=attacker AND the
        // attacker's account-local pkpData entry (this is the stale row a pre-fix
        // hijack would have left behind).
        vm.prank(stranger);
        writes.newChainSecuredAccount("attacker", "attacker");
        uint256 attackerHash = apiKeyHashOf(stranger);
        address pkpAddr = address(0xBEEF);
        vm.prank(stranger);
        writes.registerWalletDerivation(attackerHash, pkpAddr, 42, "a", "a");

        // Sanity: verify our slot math matches the contract's own getter before
        // we rely on vm.store — guards against a silent storage-layout drift.
        assertEq(
            uint256(vm.load(address(views_), _pkpOwnerSlot(pkpAddr))),
            attackerHash
        );

        // Simulate the post-backfill truth: the VICTIM was the real first owner.
        uint256 victimHash = apiKeyHashOf(user);
        vm.store(address(views_), _pkpOwnerSlot(pkpAddr), bytes32(victimHash));
        assertEq(views_.getPkpOwnerMaster(pkpAddr), victimHash);

        // The node's signing path calls getWalletDerivation with the caller's
        // account hash. The attacker still has a local entry, but the view now
        // fails closed because the wallet is owned by another account — this is
        // what neutralizes a hijack that already happened before the upgrade.
        vm.expectRevert(
            abi.encodeWithSelector(
                AppStorage.InvalidRequest.selector,
                "PKP owned by another account"
            )
        );
        views_.getWalletDerivation(attackerHash, pkpAddr);
    }

    function test_getWalletDerivation_legacyUnboundStillReadable() public {
        // A pre-migration wallet has a local entry but no owner binding yet
        // (owner==0). It must keep resolving so signing doesn't break in the
        // window between the facet upgrade and the backfill run.
        vm.prank(stranger);
        writes.newChainSecuredAccount("u", "u");
        uint256 hash = apiKeyHashOf(stranger);
        address pkpAddr = address(0xBEEF);
        vm.prank(stranger);
        writes.registerWalletDerivation(hash, pkpAddr, 42, "a", "a");

        // Clear the binding to reproduce the not-yet-backfilled legacy state.
        vm.store(address(views_), _pkpOwnerSlot(pkpAddr), bytes32(uint256(0)));
        assertEq(views_.getPkpOwnerMaster(pkpAddr), 0);

        // owner==0 falls through: the wallet still resolves for its account.
        assertEq(views_.getWalletDerivation(hash, pkpAddr), 42);
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
