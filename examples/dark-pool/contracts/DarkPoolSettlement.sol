// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @title DarkPoolSettlement
/// @notice On-chain settlement for a confidential sealed-bid batch auction.
///
/// Orders never touch this contract. They are submitted encrypted, stored as
/// ciphertext off-chain, and matched blind inside a Lit Action running in a TEE.
/// The action computes a single uniform clearing price for the epoch and signs
/// the resulting fills with a key derived from its own IPFS CID
/// (`Lit.Actions.getLitActionPrivateKey()`). This contract pins that key as
/// `matcher` at deploy time: edit the action by a byte and its CID changes, its
/// derived address changes, and this contract stops trusting its settlements.
///
/// Custody is modelled as internal balances. Traders `depositBase` /
/// `depositQuote` before an epoch closes; `settleEpoch` moves those internal
/// balances according to the signed fills at the clearing price; traders
/// `withdraw` afterwards. Unfilled and over-collateralised amounts simply remain
/// as a withdrawable balance.
///
/// PRICE CONVENTION: `clearingPx` is quote smallest-units per ONE base
/// smallest-unit, scaled by `PRICE_SCALE` (1e18). The cost in quote units of
/// `qty` base units is `qty * clearingPx / PRICE_SCALE`. The match action MUST
/// use the identical convention when it computes fills and builds the digest.
contract DarkPoolSettlement {
    using SafeERC20 for IERC20;
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    uint256 public constant PRICE_SCALE = 1e18;

    /// @notice The two assets of the single trading pair. Price is quote/base.
    IERC20 public immutable baseToken;
    IERC20 public immutable quoteToken;

    /// @notice keccak256(bytes(pair)) — e.g. keccak256("BASE/QUOTE"). Bound into
    ///         the settlement digest so a signature for one pool can't be
    ///         replayed against another.
    bytes32 public immutable pairHash;

    /// @notice Address derived from the match action's IPFS CID. Only its
    ///         signatures are accepted by `settleEpoch`.
    address public immutable matcher;

    /// @notice Internal escrow balances, credited by deposit, moved by
    ///         settlement, drained by withdraw.
    mapping(address => uint256) public baseBalance;
    mapping(address => uint256) public quoteBalance;

    /// @notice Each epoch may be settled exactly once.
    mapping(uint256 => bool) public epochSettled;

    /// @notice One matched fill in a batch. `quantity` is in base smallest-units.
    ///         All fills in an epoch execute at the same `clearingPx`.
    struct Fill {
        address trader;
        bool isBuy; // true: buys `quantity` base, pays quote; false: sells base
        uint256 quantity;
    }

    error EpochAlreadySettled();
    error InvalidMatcherSignature();
    error ConservationViolated(); // base bought != base sold
    error ZeroAmount();

    event BaseDeposited(address indexed trader, uint256 amount);
    event QuoteDeposited(address indexed trader, uint256 amount);
    event BaseWithdrawn(address indexed trader, uint256 amount);
    event QuoteWithdrawn(address indexed trader, uint256 amount);
    event EpochSettled(uint256 indexed epoch, uint256 clearingPx, uint256 fillCount);
    event Filled(uint256 indexed epoch, address indexed trader, bool isBuy, uint256 quantity, uint256 quoteAmount);

    constructor(IERC20 baseToken_, IERC20 quoteToken_, string memory pair_, address matcher_) {
        baseToken = baseToken_;
        quoteToken = quoteToken_;
        pairHash = keccak256(bytes(pair_));
        matcher = matcher_;
    }

    // -----------------------------------------------------------------------
    // Escrow
    // -----------------------------------------------------------------------

    /// @notice Deposit base tokens to back sell orders. Caller must approve first.
    function depositBase(uint256 amount) external {
        if (amount == 0) revert ZeroAmount();
        baseToken.safeTransferFrom(msg.sender, address(this), amount);
        baseBalance[msg.sender] += amount;
        emit BaseDeposited(msg.sender, amount);
    }

    /// @notice Deposit quote tokens to back buy orders. Caller must approve first.
    function depositQuote(uint256 amount) external {
        if (amount == 0) revert ZeroAmount();
        quoteToken.safeTransferFrom(msg.sender, address(this), amount);
        quoteBalance[msg.sender] += amount;
        emit QuoteDeposited(msg.sender, amount);
    }

    function withdrawBase(uint256 amount) external {
        baseBalance[msg.sender] -= amount; // reverts on underflow (0.8.x)
        baseToken.safeTransfer(msg.sender, amount);
        emit BaseWithdrawn(msg.sender, amount);
    }

    function withdrawQuote(uint256 amount) external {
        quoteBalance[msg.sender] -= amount;
        quoteToken.safeTransfer(msg.sender, amount);
        emit QuoteWithdrawn(msg.sender, amount);
    }

    // -----------------------------------------------------------------------
    // Settlement
    // -----------------------------------------------------------------------

    /// @notice Apply a batch of matcher-signed fills at a single clearing price.
    /// @dev The digest is the EIP-191 hash of
    ///      keccak256(abi.encode(epoch, pairHash, clearingPx,
    ///                            keccak256(abi.encode(fills)),
    ///                            address(this), block.chainid)).
    ///      The match action signs the same bytes with its CID-derived key.
    function settleEpoch(
        uint256 epoch,
        uint256 clearingPx,
        Fill[] calldata fills,
        bytes calldata signature
    ) external {
        if (epochSettled[epoch]) revert EpochAlreadySettled();

        bytes32 digest = keccak256(
            abi.encode(
                epoch,
                pairHash,
                clearingPx,
                keccak256(abi.encode(fills)),
                address(this),
                block.chainid
            )
        ).toEthSignedMessageHash();

        if (digest.recover(signature) != matcher) revert InvalidMatcherSignature();

        // Mark settled before moving funds (checks-effects-interactions; the
        // moves below are internal-balance updates, but keep the discipline).
        epochSettled[epoch] = true;

        uint256 baseBought;
        uint256 baseSold;

        for (uint256 i = 0; i < fills.length; i++) {
            Fill calldata f = fills[i];
            uint256 quoteAmount = (f.quantity * clearingPx) / PRICE_SCALE;

            if (f.isBuy) {
                // Buyer pays quote, receives base.
                quoteBalance[f.trader] -= quoteAmount; // reverts if under-collateralised
                baseBalance[f.trader] += f.quantity;
                baseBought += f.quantity;
            } else {
                // Seller delivers base, receives quote.
                baseBalance[f.trader] -= f.quantity;
                quoteBalance[f.trader] += quoteAmount;
                baseSold += f.quantity;
            }

            emit Filled(epoch, f.trader, f.isBuy, f.quantity, quoteAmount);
        }

        // Uniform-price call auction must clear equal base on both sides; quote
        // conservation follows because every fill uses the same clearingPx.
        if (baseBought != baseSold) revert ConservationViolated();

        emit EpochSettled(epoch, clearingPx, fills.length);
    }
}
