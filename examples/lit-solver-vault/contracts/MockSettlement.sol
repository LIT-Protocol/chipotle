// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title MockSettlement
/// @notice Stand-in for an intent-system settlement contract (Across SpokePool,
///         UniswapX reactor, an ERC-7683 settler, etc.). It holds the canonical
///         record of what a given order actually is: who it pays, in what token,
///         for how much.
///
///         The policy Lit Action reads orders from this contract on-chain and
///         binds every fill to what the order *actually says*. That on-chain
///         read is the trust anchor for the recipient-binding check — a
///         compromised solver bot can ask the action to pay an attacker, but it
///         can't rewrite the order here, so the action catches the mismatch.
///
///         In a real integration you delete this file and point the action at
///         the real settlement contract's order/deposit accessor. The shape is
///         the same: id -> (recipient, token, amount).
contract MockSettlement {
    struct Order {
        address recipient;
        address token;
        uint256 amount;
        bool exists;
    }

    mapping(bytes32 => Order) public orders;

    event OrderPosted(bytes32 indexed id, address recipient, address token, uint256 amount);

    /// @notice Post an order the solver can fill. Permissionless on purpose —
    ///         in this demo the "intent stream" is just whatever's been posted.
    function postOrder(
        bytes32 id,
        address recipient,
        address token,
        uint256 amount
    ) external {
        orders[id] = Order(recipient, token, amount, true);
        emit OrderPosted(id, recipient, token, amount);
    }

    /// @notice Read an order. The policy action calls this via eth_call to learn
    ///         the canonical recipient/token/amount and bind the fill to them.
    function getOrder(bytes32 id)
        external
        view
        returns (address recipient, address token, uint256 amount, bool exists)
    {
        Order storage o = orders[id];
        return (o.recipient, o.token, o.amount, o.exists);
    }
}
