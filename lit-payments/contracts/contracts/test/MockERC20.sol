// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.28;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// @notice Minimal ERC-20 used only in tests. Mints to whoever asks.
contract MockERC20 is ERC20 {
    constructor() ERC20("Mock LITKEY", "mLITKEY") {}

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}
