// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// @title MockUSDC
/// @notice 6-decimal test stablecoin used as the solver's inventory in this
///         example. Mints the full initial supply to the deployer, who then
///         funds the SolverVault. Not for production — it's a faucet token.
contract MockUSDC is ERC20 {
    constructor(uint256 initialSupply) ERC20("Mock USD Coin", "mUSDC") {
        _mint(msg.sender, initialSupply);
    }

    function decimals() public pure override returns (uint8) {
        return 6;
    }

    /// @notice Open faucet so anyone poking at the demo can get inventory.
    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}
