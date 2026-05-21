// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.28;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

/// @title LitkeyPaymentGateway
/// @notice Single-entrypoint contract for paying with LITKEY and getting Lit
/// Stripe credit. Tokens are pulled from `msg.sender` and forwarded to the
/// company treasury in a single transaction; the `Payment` event names the
/// Lit-account wallet to credit, which the off-chain listener maps to a Stripe
/// customer.
///
/// The contract holds no state and no funds. It is intentionally immutable —
/// if behavior needs to change, deploy a new contract and update the listener
/// and dashboard to point at it.
contract LitkeyPaymentGateway {
    using SafeERC20 for IERC20;

    /// @notice The LITKEY ERC-20 token this gateway accepts.
    IERC20 public immutable litkey;

    /// @notice Destination for swept LITKEY. Set once at deploy; expected to be
    /// the company Safe on Base.
    address public immutable treasury;

    /// @notice Emitted on a successful payment.
    /// @param wallet The Lit-account wallet the credit should be applied to.
    ///        May differ from `payer` (e.g., user pays from a Metamask wallet
    ///        but their Lit account is keyed by a different wallet).
    /// @param payer `msg.sender`, the wallet that actually sent LITKEY.
    /// @param amount LITKEY transferred, in the token's smallest unit.
    event Payment(
        address indexed wallet,
        address indexed payer,
        uint256 amount
    );

    error InvalidTreasury();
    error InvalidWallet();
    error InvalidAmount();

    constructor(IERC20 _litkey, address _treasury) {
        if (_treasury == address(0)) revert InvalidTreasury();
        litkey = _litkey;
        treasury = _treasury;
    }

    /// @notice Pull `amount` LITKEY from the caller, forward to `treasury`,
    /// and emit a `Payment` event naming `wallet` as the account to credit.
    ///
    /// Requires the caller to have approved this contract for at least
    /// `amount` LITKEY beforehand.
    /// @param amount LITKEY to transfer.
    /// @param wallet The Lit-account wallet to credit off-chain.
    function pay(uint256 amount, address wallet) external {
        if (wallet == address(0)) revert InvalidWallet();
        if (amount == 0) revert InvalidAmount();
        litkey.safeTransferFrom(msg.sender, treasury, amount);
        emit Payment(wallet, msg.sender, amount);
    }
}
