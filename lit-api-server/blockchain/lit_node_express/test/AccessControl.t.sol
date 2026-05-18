// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {BaseTest} from "./helpers/BaseTest.sol";
import {AppStorage} from "../contracts/AccountConfigFacets/AppStorage.sol";
import {NotContractOwner} from "../libraries/LibDiamond.sol";

/// @notice Verifies the gating predicates around APIConfigFacet and BillingFacet.
///         The Writes/Views facets gate via account-level checks and are covered
///         in Accounts.t.sol.
contract AccessControlTest is BaseTest {
    // -------- APIConfigFacet --------

    function test_setApiPayers_strangerReverts() public {
        address[] memory empty = new address[](0);
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.OnlyApiPayerOrOwner.selector, stranger)
        );
        apiConfig.setApiPayers(empty);
    }

    function test_setApiPayers_regularApiPayerReverts() public {
        // setApiPayers is gated to owner OR adminApiPayer only — even an existing
        // api payer (apiPayer) is rejected, to prevent hostile takeover.
        address[] memory empty = new address[](0);
        vm.prank(apiPayer);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.OnlyApiPayerOrOwner.selector, apiPayer)
        );
        apiConfig.setApiPayers(empty);
    }

    function test_setApiPayers_owner() public {
        address[] memory next = new address[](1);
        next[0] = user;
        vm.prank(owner);
        apiConfig.setApiPayers(next);
        address[] memory got = views_.api_payers();
        assertEq(got.length, 1);
        assertEq(got[0], user);
    }

    function test_setApiPayers_adminApiPayer() public {
        address[] memory next = new address[](1);
        next[0] = user;
        vm.prank(adminApiPayer);
        apiConfig.setApiPayers(next);
        assertEq(views_.api_payers()[0], user);
    }

    function test_setConfigOperator_strangerReverts() public {
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.OnlyConfigOperatorOrOwner.selector, stranger)
        );
        apiConfig.setConfigOperator(user);
    }

    function test_setConfigOperator_owner() public {
        vm.prank(owner);
        apiConfig.setConfigOperator(user);
        assertEq(views_.configOperator(), user);
    }

    function test_setConfigOperator_currentConfigOperator() public {
        // Owner is the initial configOperator. Reassign to user, then user should be able to.
        vm.prank(owner);
        apiConfig.setConfigOperator(user);
        vm.prank(user);
        apiConfig.setConfigOperator(stranger);
        assertEq(views_.configOperator(), stranger);
    }

    function test_setRequestedApiPayerCount_strangerReverts() public {
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.OnlyApiPayerOrOwner.selector, stranger)
        );
        apiConfig.setRequestedApiPayerCount(7);
    }

    function test_setRequestedApiPayerCount_apiPayerOk() public {
        vm.prank(apiPayer);
        apiConfig.setRequestedApiPayerCount(7);
        assertEq(views_.requestedApiPayerCount(), 7);
    }

    function test_serverTrigger_onlyOwner() public {
        vm.prank(stranger);
        vm.expectRevert(abi.encodeWithSelector(NotContractOwner.selector, stranger, owner));
        apiConfig.serverTrigger(123);

        // Even an api payer cannot.
        vm.prank(apiPayer);
        vm.expectRevert(abi.encodeWithSelector(NotContractOwner.selector, apiPayer, owner));
        apiConfig.serverTrigger(123);

        vm.prank(owner);
        apiConfig.serverTrigger(123); // ok
    }

    // -------- BillingFacet --------

    function test_setPricing_strangerReverts() public {
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.OnlyApiPayerOrPricingOperator.selector, stranger)
        );
        billing.setPricing(2, 99);
    }

    function test_setPricing_apiPayerOk() public {
        vm.prank(apiPayer);
        billing.setPricing(2, 99);
        assertEq(billing.getPricing(2), 99);
    }

    function test_setPricing_pricingOperatorOk() public {
        // pricingOperator defaults to the owner from the constructor.
        vm.prank(owner);
        billing.setPricing(2, 99);
        assertEq(billing.getPricing(2), 99);
    }

    function test_setPricingOperator_onlyOwner() public {
        vm.prank(stranger);
        vm.expectRevert(abi.encodeWithSelector(NotContractOwner.selector, stranger, owner));
        billing.setPricingOperator(user);

        // Existing pricingOperator (= owner) can. After reassignment, the previous
        // pricingOperator (now non-owner) loses the ability.
        vm.prank(owner);
        billing.setPricingOperator(user);
        assertEq(views_.pricingOperator(), user);
    }

    function test_creditAndDebitApiKey_apiPayerFlow() public {
        // Create a managed account so apiPayer has access.
        uint256 hash = uint256(keccak256("billed"));
        vm.prank(apiPayer);
        writes.newAccount(hash, true, "managed", "billed", user);

        vm.prank(apiPayer);
        billing.creditApiKey(hash, 100);

        vm.prank(apiPayer);
        billing.debitApiKey(hash, 40);

        // Can't debit more than the remaining balance.
        vm.prank(apiPayer);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.InsufficientBalance.selector, hash, 1000)
        );
        billing.debitApiKey(hash, 1000);
    }

    function test_debitApiKey_strangerReverts() public {
        vm.prank(apiPayer);
        writes.newAccount(uint256(keccak256("k")), true, "x", "x", user);

        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.OnlyApiPayerOrPricingOperator.selector, stranger)
        );
        billing.debitApiKey(uint256(keccak256("k")), 1);
    }
}
