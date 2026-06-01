// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// @title TestToken
/// @notice Minimal mintable ERC-20 used to stand in for the base and quote
///         assets in the dark-pool demo. Anyone can mint — this is a testnet
///         toy, not a real token. The deploy script mints starting balances to
///         the demo traders so they have something to escrow.
contract TestToken is ERC20 {
    constructor(string memory name_, string memory symbol_) ERC20(name_, symbol_) {}

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}
