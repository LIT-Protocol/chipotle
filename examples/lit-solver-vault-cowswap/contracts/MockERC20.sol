// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// @notice Faucet ERC20 used for the demo. The deploy script mints the trader
///         their sell token and mints the vault its buy-token inventory. Two
///         instances stand in for, e.g., a stablecoin (6 decimals) the user
///         sells and WETH (18 decimals) the solver holds — to show the
///         clearing-price / settlement math is decimal-agnostic.
contract MockERC20 is ERC20 {
    uint8 private immutable _decimals;

    constructor(string memory name_, string memory symbol_, uint8 decimals_)
        ERC20(name_, symbol_)
    {
        _decimals = decimals_;
    }

    function decimals() public view override returns (uint8) {
        return _decimals;
    }

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}
