/// @title AccountConfig
/// @author Brendon Paul
/// @notice Diamond-style AccountConfig: storage in LibAccountConfigStorage, logic in AccountConfigFacet (mutable) and AccountConfigViews (read-only).
/// @notice This contract composes both facets so a single deployment exposes the full interface.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {AccountConfigFacet} from "./AccountConfigFacet.sol";
import {AccountConfigViews} from "./AccountConfigViews.sol";

contract AccountConfig is AccountConfigFacet, AccountConfigViews {
    constructor() {
        initializeAccountConfig();
    }
}
