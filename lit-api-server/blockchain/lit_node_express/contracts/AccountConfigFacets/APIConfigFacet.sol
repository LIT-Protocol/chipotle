/// @title AccountAPIProcess
/// @author Brendon Paul
/// @notice Process API calls for AccountConfig diamond.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {AppStorage} from "./AppStorage.sol";
import {SecurityLib} from "./SecurityLib.sol";

contract APIConfigFacet {
    using EnumerableSet for EnumerableSet.UintSet;
    using EnumerableSet for EnumerableSet.AddressSet;

    function setConfigOperator(address newConfigOperator) public {
        SecurityLib.revertIfNotConfigOperatorOrOwner(msg.sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.configOperator = newConfigOperator;
    }

    function setRequestedApiPayerCount(
        uint256 newRequestedApiPayerCount
    ) public {
        SecurityLib.revertIfNotApiPayerOrOwner(msg.sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.requestedApiPayerCount = newRequestedApiPayerCount;
    }

    function setAdminApiPayerAccount(address newAdminApiPayerAccount) public {
        SecurityLib.revertIfNotApiPayerOrOwner(msg.sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.adminApiPayerAccount = newAdminApiPayerAccount;
    }

    // setApiPayers is used to add new signers (accounts that pay for state mutation made by api calls) to the list of api payers.
    function setApiPayers(address[] memory newApiPayers) public {
        SecurityLib.revertIfNotApiPayerOrOwner(msg.sender);

        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();

        s.api_payers.clear();

        for (uint256 i = 0; i < newApiPayers.length; i++) {
            s.api_payers.add(newApiPayers[i]);
        }
    }

    function setRebalanceAmount(uint256 newRebalanceAmount) public {
        SecurityLib.revertIfNotApiPayerOrOwner(msg.sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.rebalanceAmount = newRebalanceAmount;
    }
}
