/// @title AccountAPIProcess
/// @author Brendon Paul
/// @notice Process API calls for AccountConfig diamond.

// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {AppStorage} from "./AppStorage.sol";
import {SecurityLib} from "./SecurityLib.sol";
import {LibDiamond} from "../../libraries/LibDiamond.sol";

contract APIConfigFacet {
    using EnumerableSet for EnumerableSet.UintSet;
    using EnumerableSet for EnumerableSet.AddressSet;

    event ServerTriggered(uint256 value, address indexed sender);

    event ConfigOperatorUpdated(address indexed newConfigOperator);
    event ApiPayersUpdated(address[] newApiPayers);
    event AdminApiPayerUpdated(address indexed newAdminApiPayerAccount);
    event RebalanceAmountUpdated(uint256 newRebalanceAmount);
    event RequestedApiPayerCountUpdated(uint256 newCount);

    function setConfigOperator(address newConfigOperator) public {
        SecurityLib.revertIfNotConfigOperatorOrOwner(msg.sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.configOperator = newConfigOperator;
        emit ConfigOperatorUpdated(newConfigOperator);
    }

    function setRequestedApiPayerCount(
        uint256 newRequestedApiPayerCount
    ) public {
        SecurityLib.revertIfNotApiPayerOrOwner(msg.sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.requestedApiPayerCount = newRequestedApiPayerCount;
        emit RequestedApiPayerCountUpdated(newRequestedApiPayerCount);
    }

    // Owner-only: the admin api payer outranks every regular api_payer
    // (it alone gates setApiPayers), so letting any api_payer set it would let
    // a single api_payer self-promote to admin and seize the whole payer set.
    // Deploy/rotation always sends this from the diamond owner, never an
    // api_payer, so owner-only closes the self-promotion path with no
    // legitimate caller affected.
    function setAdminApiPayerAccount(address newAdminApiPayerAccount) public {
        SecurityLib.revertIfNotOwner(msg.sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.adminApiPayerAccount = newAdminApiPayerAccount;
        emit AdminApiPayerUpdated(newAdminApiPayerAccount);
    }

    // setApiPayers replaces the entire api_payer set (clear + rebuild), and api
    // payers can create accounts, move balances, register PKPs, and convert
    // accounts. Granting/revoking that role is therefore restricted to the
    // diamond owner ONLY. It used to also allow the adminApiPayerAccount, but
    // combined with a reachable setAdminApiPayerAccount that formed a privilege-
    // escalation chain (any payer -> admin payer -> rewrite the whole payer set).
    function setApiPayers(address[] memory newApiPayers) public {
        SecurityLib.revertIfNotOwner(msg.sender);

        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();

        s.api_payers.clear();

        for (uint256 i = 0; i < newApiPayers.length; i++) {
            s.api_payers.add(newApiPayers[i]);
        }
        emit ApiPayersUpdated(newApiPayers);
    }

    function setRebalanceAmount(uint256 newRebalanceAmount) public {
        SecurityLib.revertIfNotApiPayerOrOwner(msg.sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.rebalanceAmount = newRebalanceAmount;
        emit RebalanceAmountUpdated(newRebalanceAmount);
    }

    /// @notice Trigger a server restart signal. Only callable by the diamond owner.
    /// @param value Arbitrary uint256 value stored on-chain and emitted in the event.
    function serverTrigger(uint256 value) public {
        LibDiamond.enforceIsContractOwner();
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.serverTriggerValue = value;
        emit ServerTriggered(value, msg.sender);
    }
}
