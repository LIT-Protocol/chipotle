// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @title ConsensusOracle
/// @notice Registry of off-chain readings attested by a PKP. The PKP only
///         signs a reading after a Lit Action confirmed that three independent
///         RPC providers (Infura, Alchemy, QuickNode) returned the exact same
///         bytes for the same `(target, callData)` at the same block. Anyone
///         can submit the signed reading on-chain.
///
/// Consumers query `latest(target, callData)` to read the most recent attested
/// return data plus its source-chain block timestamp.
contract ConsensusOracle {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    /// @notice PKP address authorised to sign readings.
    address public immutable signer;

    struct Reading {
        bytes data;
        uint64 observedAt;   // source-chain block.timestamp at the read block
        uint64 submittedAt;  // this-chain block.timestamp at submit
    }

    /// @dev keyed on keccak256(abi.encode(target, callData))
    mapping(bytes32 => Reading) public readings;

    error Expired();
    error StaleReading();
    error InvalidSignature();
    error EmptyReturnData();

    event ReadingSubmitted(
        bytes32 indexed key,
        address indexed target,
        bytes callData,
        bytes returnData,
        uint64 observedAt
    );

    constructor(address signer_) {
        signer = signer_;
    }

    function readingKey(address target, bytes calldata callData)
        public
        pure
        returns (bytes32)
    {
        return keccak256(abi.encode(target, callData));
    }

    /// @notice Submit a PKP-attested view-function reading.
    function submit(
        address target,
        bytes calldata callData,
        bytes calldata returnData,
        uint256 observedAt,
        uint256 deadline,
        bytes calldata signature
    ) external {
        if (block.timestamp > deadline) revert Expired();
        if (returnData.length == 0) revert EmptyReturnData();

        bytes32 digest = keccak256(
            abi.encode(
                target,
                callData,
                returnData,
                observedAt,
                deadline,
                address(this),
                block.chainid
            )
        ).toEthSignedMessageHash();

        if (digest.recover(signature) != signer) revert InvalidSignature();

        bytes32 k = readingKey(target, callData);
        if (uint64(observedAt) <= readings[k].observedAt) revert StaleReading();

        readings[k] = Reading({
            data: returnData,
            observedAt: uint64(observedAt),
            submittedAt: uint64(block.timestamp)
        });

        emit ReadingSubmitted(k, target, callData, returnData, uint64(observedAt));
    }

    /// @notice Read the most recent attested return-data for a (target, callData).
    function latest(address target, bytes calldata callData)
        external
        view
        returns (bytes memory data, uint64 observedAt, uint64 submittedAt)
    {
        Reading memory r = readings[readingKey(target, callData)];
        return (r.data, r.observedAt, r.submittedAt);
    }
}
