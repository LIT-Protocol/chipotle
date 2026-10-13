
// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {AppStorage} from "../../contracts/AccountConfigFacets/AppStorage.sol";
import {SecurityLib} from "../../contracts/AccountConfigFacets/SecurityLib.sol";
import {LibDiamond} from "../../libraries/LibDiamond.sol";

contract LegacyAPIConfigFacet {
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

    function setAdminApiPayerAccount(address newAdminApiPayerAccount) public {
        SecurityLib.revertIfNotOwner(msg.sender);
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.adminApiPayerAccount = newAdminApiPayerAccount;
        emit AdminApiPayerUpdated(newAdminApiPayerAccount);
    }

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

    function serverTrigger(uint256 value) public {
        LibDiamond.enforceIsContractOwner();
        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        s.serverTriggerValue = value;
        emit ServerTriggered(value, msg.sender);
    }
}
