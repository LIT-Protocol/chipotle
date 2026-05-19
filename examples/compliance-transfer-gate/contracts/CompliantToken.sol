// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @title CompliantToken
/// @notice ERC-20 where every transfer requires a signed authorization from a
///         designated PKP "compliance oracle". The PKP runs a Lit Action that
///         screens the recipient address against a sanctions / risk API and
///         only signs when the recipient passes.
///
/// The plain ERC-20 `transfer` and `transferFrom` functions are disabled —
/// callers must use `transferWithAuth` and supply a signature produced by the
/// off-chain Lit Action.
contract CompliantToken is ERC20 {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    /// @notice Address of the PKP that signs compliance authorizations.
    address public immutable complianceOracle;

    /// @notice Replay-protection: each (from, nonce) pair may be used once.
    mapping(address => mapping(bytes32 => bool)) public usedNonces;

    error AuthorizationExpired();
    error NonceAlreadyUsed();
    error InvalidComplianceSignature();
    error TransferRequiresAuthorization();

    event CompliantTransfer(
        address indexed from,
        address indexed to,
        uint256 amount,
        bytes32 indexed nonce
    );

    constructor(
        string memory name_,
        string memory symbol_,
        uint256 initialSupply,
        address oracle_
    ) ERC20(name_, symbol_) {
        complianceOracle = oracle_;
        _mint(msg.sender, initialSupply);
    }

    /// @notice Transfer tokens after presenting a fresh signature from the
    ///         compliance oracle. The signature is over the EIP-191 hash of
    ///         (from, to, amount, nonce, deadline, this contract, chainid).
    function transferWithAuth(
        address to,
        uint256 amount,
        bytes32 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external returns (bool) {
        if (block.timestamp > deadline) revert AuthorizationExpired();
        if (usedNonces[msg.sender][nonce]) revert NonceAlreadyUsed();

        bytes32 digest = keccak256(
            abi.encode(
                msg.sender,
                to,
                amount,
                nonce,
                deadline,
                address(this),
                block.chainid
            )
        ).toEthSignedMessageHash();

        address signer = digest.recover(signature);
        if (signer != complianceOracle) revert InvalidComplianceSignature();

        usedNonces[msg.sender][nonce] = true;
        _transfer(msg.sender, to, amount);

        emit CompliantTransfer(msg.sender, to, amount, nonce);
        return true;
    }

    /// @dev Block the standard ERC-20 transfer paths so all flow goes through
    ///      `transferWithAuth`. Minting and burning still go through `_update`
    ///      and are unaffected because they bypass `transfer`/`transferFrom`.
    function transfer(address, uint256) public pure override returns (bool) {
        revert TransferRequiresAuthorization();
    }

    function transferFrom(
        address,
        address,
        uint256
    ) public pure override returns (bool) {
        revert TransferRequiresAuthorization();
    }
}
