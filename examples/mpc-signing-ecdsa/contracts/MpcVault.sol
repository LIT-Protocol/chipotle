// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @title MpcVault
/// @notice A vault controlled by a threshold-ECDSA key whose shares are split
///         between a Lit Action and the user (2-of-3 by default: Lit + the
///         user's hot share + a cold recovery share; also runs 2-of-2). Neither
///         party can sign alone; every `exec` requires a signature produced by
///         the interactive MPC protocol between a signing quorum.
///
/// The contract has no idea MPC was involved: the signer is a normal
/// secp256k1 address and the signature verifies with plain `ecrecover`. That
/// is the entire point of doing real threshold ECDSA rather than an on-chain
/// multisig — the key is portable and looks ordinary on-chain.
contract MpcVault {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    /// @notice The MPC-derived address authorized to move funds. Pinned at
    ///         deploy from the public key produced by the distributed keygen.
    address public immutable signer;

    /// @notice Monotonic nonce; part of every signed digest to prevent replay.
    uint256 public nonce;

    error InvalidSignature();
    error CallFailed();

    event Executed(uint256 indexed nonce, address indexed to, uint256 value, bytes data);

    constructor(address signer_) {
        signer = signer_;
    }

    /// @notice The digest the MPC key must sign to authorize a given call.
    ///         Bound to this contract, this chain, and the current nonce.
    function digest(address to, uint256 value, bytes calldata data) public view returns (bytes32) {
        return keccak256(abi.encode(address(this), block.chainid, nonce, to, value, data));
    }

    /// @notice Execute a call authorized by the threshold MPC signature.
    function exec(address to, uint256 value, bytes calldata data, bytes calldata signature) external {
        bytes32 ethSigned = digest(to, value, data).toEthSignedMessageHash();
        if (ethSigned.recover(signature) != signer) revert InvalidSignature();

        nonce++;

        (bool ok, ) = to.call{value: value}(data);
        if (!ok) revert CallFailed();

        emit Executed(nonce - 1, to, value, data);
    }

    receive() external payable {}
}
