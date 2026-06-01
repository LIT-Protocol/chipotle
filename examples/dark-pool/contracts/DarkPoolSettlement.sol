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
/// The action verifies each order's trader signature, computes a single uniform
/// clearing price for the epoch, and signs the resulting fills with a key derived
/// from its own IPFS CID (`Lit.Actions.getLitActionPrivateKey()`). This contract
/// pins that key as `matcher` at deploy time: edit the action by a byte and its
/// CID changes, its derived address changes, and this contract stops trusting its
/// settlements.
///
/// CUSTODY — escrow is locked PER EPOCH. A trader `depositBase`/`depositQuote`s
/// against a specific epoch; those funds are locked until that epoch settles, so
/// a matched trader cannot withdraw out from under a pending settlement. On
/// settlement, spent escrow becomes the counterparty's withdrawable proceeds, and
/// any unspent escrow becomes withdrawable by the trader once the epoch is
/// settled. This bounds the blast radius of any single order to that epoch's
/// escrow.
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

    /// @notice keccak256(bytes(pair)) — bound into the settlement digest and the
    ///         per-order trader signature so neither can be replayed across pools.
    bytes32 public immutable pairHash;

    /// @notice Address derived from the match action's IPFS CID. Only its
    ///         signatures are accepted by `settleEpoch`.
    address public immutable matcher;

    /// @notice Escrow locked to a specific epoch: epoch => trader => amount.
    ///         Locked until that epoch settles.
    mapping(uint256 => mapping(address => uint256)) public baseEscrow;
    mapping(uint256 => mapping(address => uint256)) public quoteEscrow;

    /// @notice Settlement proceeds, withdrawable any time.
    mapping(address => uint256) public baseProceeds;
    mapping(address => uint256) public quoteProceeds;

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
    error EpochNotSettled();
    error InvalidMatcherSignature();
    error ConservationViolated(); // base bought != base sold
    error ZeroAmount();

    event BaseDeposited(uint256 indexed epoch, address indexed trader, uint256 amount);
    event QuoteDeposited(uint256 indexed epoch, address indexed trader, uint256 amount);
    event EscrowRefunded(uint256 indexed epoch, address indexed trader, uint256 base, uint256 quote);
    event ProceedsWithdrawn(address indexed trader, uint256 base, uint256 quote);
    event EpochSettled(uint256 indexed epoch, uint256 clearingPx, uint256 fillCount);
    event Filled(uint256 indexed epoch, address indexed trader, bool isBuy, uint256 quantity, uint256 quoteAmount);

    constructor(IERC20 baseToken_, IERC20 quoteToken_, string memory pair_, address matcher_) {
        baseToken = baseToken_;
        quoteToken = quoteToken_;
        pairHash = keccak256(bytes(pair_));
        matcher = matcher_;
    }

    // -----------------------------------------------------------------------
    // Escrow (locked per epoch until that epoch settles)
    // -----------------------------------------------------------------------

    /// @notice Escrow base to back sell orders in `epoch`. Caller approves first.
    function depositBase(uint256 epoch, uint256 amount) external {
        if (amount == 0) revert ZeroAmount();
        if (epochSettled[epoch]) revert EpochAlreadySettled();
        baseToken.safeTransferFrom(msg.sender, address(this), amount);
        baseEscrow[epoch][msg.sender] += amount;
        emit BaseDeposited(epoch, msg.sender, amount);
    }

    /// @notice Escrow quote to back buy orders in `epoch`. Caller approves first.
    function depositQuote(uint256 epoch, uint256 amount) external {
        if (amount == 0) revert ZeroAmount();
        if (epochSettled[epoch]) revert EpochAlreadySettled();
        quoteToken.safeTransferFrom(msg.sender, address(this), amount);
        quoteEscrow[epoch][msg.sender] += amount;
        emit QuoteDeposited(epoch, msg.sender, amount);
    }

    /// @notice After an epoch settles, reclaim any escrow that wasn't spent
    ///         (unfilled or over-collateralised amounts).
    function withdrawEscrow(uint256 epoch) external {
        if (!epochSettled[epoch]) revert EpochNotSettled();
        uint256 b = baseEscrow[epoch][msg.sender];
        uint256 q = quoteEscrow[epoch][msg.sender];
        baseEscrow[epoch][msg.sender] = 0;
        quoteEscrow[epoch][msg.sender] = 0;
        if (b > 0) baseToken.safeTransfer(msg.sender, b);
        if (q > 0) quoteToken.safeTransfer(msg.sender, q);
        emit EscrowRefunded(epoch, msg.sender, b, q);
    }

    /// @notice Withdraw settlement proceeds (base bought / quote received).
    function withdrawProceeds() external {
        uint256 b = baseProceeds[msg.sender];
        uint256 q = quoteProceeds[msg.sender];
        baseProceeds[msg.sender] = 0;
        quoteProceeds[msg.sender] = 0;
        if (b > 0) baseToken.safeTransfer(msg.sender, b);
        if (q > 0) quoteToken.safeTransfer(msg.sender, q);
        emit ProceedsWithdrawn(msg.sender, b, q);
    }

    // -----------------------------------------------------------------------
    // Settlement
    // -----------------------------------------------------------------------

    /// @notice Apply a batch of matcher-signed fills at a single clearing price.
    /// @dev The digest is the EIP-191 hash of
    ///      keccak256(abi.encode(epoch, pairHash, clearingPx,
    ///                            keccak256(abi.encode(fills)),
    ///                            address(this), block.chainid)).
    ///      The match action signs the same bytes with its CID-derived key, and
    ///      has already verified every fill's trader signature off-chain.
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

        // Effects before interactions (the moves are internal-balance updates).
        epochSettled[epoch] = true;

        uint256 baseBought;
        uint256 baseSold;

        for (uint256 i = 0; i < fills.length; i++) {
            Fill calldata f = fills[i];
            uint256 quoteAmount = (f.quantity * clearingPx) / PRICE_SCALE;

            if (f.isBuy) {
                // Buyer spends locked quote escrow, receives base proceeds.
                quoteEscrow[epoch][f.trader] -= quoteAmount; // reverts if under-collateralised
                baseProceeds[f.trader] += f.quantity;
                baseBought += f.quantity;
            } else {
                // Seller delivers locked base escrow, receives quote proceeds.
                baseEscrow[epoch][f.trader] -= f.quantity;
                quoteProceeds[f.trader] += quoteAmount;
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
