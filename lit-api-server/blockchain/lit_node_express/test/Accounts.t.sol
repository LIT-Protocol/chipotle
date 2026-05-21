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
}
