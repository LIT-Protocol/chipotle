// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @title PriceOracle
/// @notice Registry of asset prices attested by a PKP. The Lit Action behind
///         this oracle polls three independent price sources (Coinbase,
///         Kraken, Bitstamp) for each `submit()`, takes the median, and signs
///         the result. The contract verifies the signature came from the
///         configured `signer` address and stores the price.
///
/// Prices are stored as raw uint256 with a `decimals` field so consumers can
/// interpret the precision. The example signs everything at 8 decimals
/// (matching Chainlink convention) — the contract is agnostic.
contract PriceOracle {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    /// @notice Address authorized to sign price attestations.
    address public immutable signer;

    struct Reading {
        uint256 price;       // raw integer with `decimals` precision
        uint8 decimals;      // typically 8
        uint64 observedAt;   // unix seconds — when the action read the sources
        uint64 submittedAt;  // unix seconds — when this contract recorded it
    }

    /// @dev keyed on keccak256(bytes(symbol))
    mapping(bytes32 => Reading) public readings;

    error Expired();
    error StaleReading();
    error InvalidSignature();
    error EmptyAsset();

    event PriceSubmitted(
        bytes32 indexed key,
        string asset,
        uint256 price,
        uint8 decimals,
        uint64 observedAt
    );

    constructor(address signer_) {
        signer = signer_;
    }

    function assetKey(string memory asset) public pure returns (bytes32) {
        return keccak256(bytes(asset));
    }

    /// @notice Submit a PKP-attested price reading.
    function submit(
        string calldata asset,
        uint256 price,
        uint8 decimals,
        uint256 observedAt,
        uint256 deadline,
        bytes calldata signature
    ) external {
        if (block.timestamp > deadline) revert Expired();
        if (bytes(asset).length == 0) revert EmptyAsset();

        bytes32 digest = keccak256(
            abi.encode(
                asset,
                price,
                decimals,
                observedAt,
                deadline,
                address(this),
                block.chainid
            )
        ).toEthSignedMessageHash();

        if (digest.recover(signature) != signer) revert InvalidSignature();

        bytes32 k = assetKey(asset);
        if (uint64(observedAt) <= readings[k].observedAt) revert StaleReading();

        readings[k] = Reading({
            price: price,
            decimals: decimals,
            observedAt: uint64(observedAt),
            submittedAt: uint64(block.timestamp)
        });

        emit PriceSubmitted(k, asset, price, decimals, uint64(observedAt));
    }

    /// @notice Read the most recent attested price for an asset.
    function latest(string calldata asset)
        external
        view
        returns (uint256 price, uint8 decimals, uint64 observedAt, uint64 submittedAt)
    {
        Reading memory r = readings[assetKey(asset)];
        return (r.price, r.decimals, r.observedAt, r.submittedAt);
    }
}
