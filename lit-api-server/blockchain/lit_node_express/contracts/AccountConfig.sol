/// @title AccountConfig
/// @author Brendon Paul
/// @notice Pre-Diamond-style AccountConfig: storage in LibAccountConfigStorage, logic in AccountConfigWrite (mutable) and AccountConfigViews (read-only).
/// @notice This contract composes both facets so a single deployment exposes the full interface.

// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {AppStorage} from "./AccountConfigFacets/AppStorage.sol";
import {
    EnumerableSet
} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {LibDiamond} from "../libraries/LibDiamond.sol";
import {IDiamondCut} from "../interfaces/IDiamondCut.sol";
import {IDiamondLoupe} from "../interfaces/IDiamondLoupe.sol";

// When no function exists for function called
error FunctionNotFound(bytes4 _functionSelector);

contract AccountConfig {
    using EnumerableSet for EnumerableSet.AddressSet;

    constructor(
        address owner,
        IDiamondCut.FacetCut[] memory _diamondCut,
        address _init,
        bytes memory _calldata
    ) {
        LibDiamond.setContractOwner(owner);
        LibDiamond.diamondCut(_diamondCut, _init, _calldata);

        AppStorage.AccountConfigStorage storage s = AppStorage.getStorage();
        require(s.api_payers.length() == 0, "already initialized");
        s.pricingOperator = owner;
        s.configOperator = owner;
        s.pricing[1] = 1;
        s.requestedApiPayerCount = 3; // just a default for spinning up a new instance
    }

    /// @notice Reject any ETH sent directly to the contract.
    receive() external payable {
        revert();
    }

    // Find facet for function that is called and execute the
    // function if a facet is found and return any value.
    fallback() external {
        LibDiamond.DiamondStorage storage ds;
        bytes32 position = LibDiamond.DIAMOND_STORAGE_POSITION;
        // get diamond storage
        assembly {
            ds.slot := position
        }
        // get facet from function selector
        address facet = ds
            .facetAddressAndSelectorPosition[msg.sig]
            .facetAddress;
        if (facet == address(0)) {
            revert FunctionNotFound(msg.sig);
        }
        // Execute external function from facet using delegatecall and return any value.
        assembly {
            // copy function selector and any arguments
            calldatacopy(0, 0, calldatasize())
            // execute function call using the facet
            let result := delegatecall(gas(), facet, 0, calldatasize(), 0, 0)
            // get any return value
            returndatacopy(0, 0, returndatasize())
            // return any return value or error back to the caller
            switch result
            case 0 {
                revert(0, returndatasize())
            }
            default {
                return(0, returndatasize())
            }
        }
    }
}
