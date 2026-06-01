// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// @title DemoToken
/// @notice Plain 18-decimal ERC-20 with an open faucet mint, used so the
///         example can fund action-bound wallets without needing a real token.
///         Not for production — anyone can mint.
contract DemoToken is ERC20 {
    constructor(uint256 initialSupply) ERC20("Action Bound Demo", "ABD") {
        _mint(msg.sender, initialSupply);
    }

    /// @notice Open faucet so anyone poking at the demo can get tokens.
    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}
