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

    /// @notice Who may post orders. A real settlement contract is a protocol
    ///         contract an attacker can't write favorable orders into; this mock
    ///         restricts posting to its deployer so the demo's recipient-binding
    ///         claim holds (otherwise a holder of the usage key could post an
    ///         order paying themselves and have the policy bind to it).
    address public immutable poster;

    error NotPoster();

    event OrderPosted(bytes32 indexed id, address recipient, address token, uint256 amount);

    constructor() {
        poster = msg.sender;
    }

    /// @notice Post an order the solver can fill. Restricted to the deployer —
    ///         see `poster`.
    function postOrder(
        bytes32 id,
        address recipient,
        address token,
        uint256 amount
    ) external {
        if (msg.sender != poster) revert NotPoster();
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
