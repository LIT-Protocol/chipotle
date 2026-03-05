/// @title AccountConfig
/// @author Brendon Paul
/// @notice Pre-Diamond-style AccountConfig: storage in LibAccountConfigStorage, logic in AccountConfigWrite (mutable) and AccountConfigViews (read-only).
/// @notice This contract composes both facets so a single deployment exposes the full interface.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {AccountConfigWrite} from "./AccountConfigFacets/AccountConfigWrites.sol";
import {AccountConfigViews} from "./AccountConfigFacets/AccountConfigViews.sol";

contract AccountConfig is AccountConfigWrite, AccountConfigViews {
    constructor() {
        AccountConfigWrite.initializeAccountConfig();
    }
}
