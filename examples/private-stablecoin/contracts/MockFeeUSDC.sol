// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// @title MockFeeUSDC
/// @notice A fee-on-transfer token used only in tests: every transfer burns 1%
///         of the amount, so the recipient receives less than `amount`. Used to
///         prove PrivUSD's balance-delta check rejects reserves that would
///         credit more privUSD than USDC actually arrived.
contract MockFeeUSDC is ERC20 {
    constructor() ERC20("Fee USD Coin", "fUSDC") {}

    function decimals() public pure override returns (uint8) {
        return 6;
    }

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }

    function _update(address from, address to, uint256 value) internal override {
        if (from != address(0) && to != address(0)) {
            uint256 fee = value / 100; // 1% burned in transit
            super._update(from, address(0), fee);
            super._update(from, to, value - fee);
        } else {
            super._update(from, to, value);
        }
    }
}
