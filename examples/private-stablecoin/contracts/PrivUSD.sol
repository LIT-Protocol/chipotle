// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @title PrivUSD
/// @notice A compliant private stablecoin. Balances and transfers are hidden:
///         the chain stores only note *commitments* (hashes), *nullifiers*
///         (spent-note tags), and *encrypted blobs* (note contents, decryptable
///         only inside a Lit Action). No amounts or addresses appear in
///         cleartext for transfers.
///
///         There are no ZK circuits. A Lit Action — running in the Lit TEE —
///         is the prover: it reads chain state, decrypts the relevant notes,
///         checks the arithmetic and OFAC/KYC, and signs the state update with
///         its CID-derived key. This contract
///         verifies that one signature (`ecrecover`) and applies the update.
///         Edit the action by a byte and its CID — and therefore its signer
///         address — changes, so this contract stops trusting it.
///
///         Reserve proof: `reserveBacked()` is public and continuous. Anyone
///         can confirm the USDC held here covers every privUSD in circulation.
///
///         DEMO-GRADE. Real production hardening (per-recipient encryption,
///         multi-source OFAC, oracle rotation behind a multisig) is described
///         in plans/private-stablecoin.md.
contract PrivUSD {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;
    using SafeERC20 for IERC20;

    /// @notice The Lit Action's CID-derived signer. Sole authority for every
    ///         state change. Pinned at deploy from the action's IPFS CID.
    address public immutable ledgerOracle;

    /// @notice The reserve asset (USDC). Held 1:1 against privUSD in circulation.
    IERC20 public immutable reserve;

    /// @notice privUSD in circulation, in reserve base units (USDC has 6 decimals).
    uint256 public totalSupply;

    /// @notice Live note commitments. commitment = keccak256(owner, amount, salt).
    mapping(bytes32 => bool) public commitments;

    /// @notice Spent-note tags. Prevents double-spend. Unlinkable to commitments.
    mapping(bytes32 => bool) public nullifiers;

    /// @notice Replay protection on the oracle's authorizations.
    mapping(bytes32 => bool) public usedNonces;

    error AuthorizationExpired();
    error NonceAlreadyUsed();
    error InvalidOracleSignature();
    error CommitmentExists();
    error NoteAlreadySpent();
    error LengthMismatch();
    error InsufficientSupply();
    error ReserveDeltaMismatch();

    /// @dev A new note was created. The blob is the note's contents (owner,
    ///      amount, salt) encrypted to the ledger PKP — readable only inside an
    ///      authorized Lit Action. This is what makes the ledger reconstructable
    ///      from chain alone: no off-chain database.
    event NoteCreated(bytes32 indexed commitment, string encryptedBlob);

    /// @dev A note was spent. Reveals nothing about which commitment it was.
    event NoteSpent(bytes32 indexed nullifier);

    event Minted(uint256 amount);
    event Redeemed(address indexed to, uint256 amount);

    constructor(address reserve_, address oracle_) {
        reserve = IERC20(reserve_);
        ledgerOracle = oracle_;
    }

    /// @notice Public, continuous reserve proof. True iff the USDC held here
    ///         covers every privUSD in circulation. A watcher should halt the
    ///         system if this ever goes false.
    function reserveBacked() external view returns (bool) {
        return reserve.balanceOf(address(this)) >= totalSupply;
    }

    // -------------------------------------------------------------------------
    // mint: USDC -> privUSD. The action verified KYC + OFAC and that
    // depositAmount equals the sum of the new notes' amounts before signing.
    // -------------------------------------------------------------------------
    function mint(
        address depositor,
        uint256 depositAmount,
        bytes32[] calldata newCommitments,
        string[] calldata encryptedBlobs,
        bytes32 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external {
        if (newCommitments.length != encryptedBlobs.length) revert LengthMismatch();
        _checkAuth(
            keccak256(
                abi.encode(
                    "MINT",
                    depositor,
                    depositAmount,
                    newCommitments,
                    encryptedBlobs,
                    nonce,
                    deadline,
                    address(this),
                    block.chainid
                )
            ),
            nonce,
            deadline,
            signature
        );

        // Pull the reserve in. depositor must have approved this contract.
        // SafeERC20 handles non-standard (no-return) tokens; the balance-delta
        // check rejects fee-on-transfer / rebasing reserves that would credit
        // more privUSD than USDC actually arrived, breaking the 1:1 backing.
        uint256 balBefore = reserve.balanceOf(address(this));
        reserve.safeTransferFrom(depositor, address(this), depositAmount);
        uint256 received = reserve.balanceOf(address(this)) - balBefore;
        if (received != depositAmount) revert ReserveDeltaMismatch();

        _addNotes(newCommitments, encryptedBlobs);
        totalSupply += depositAmount;
        emit Minted(depositAmount);
    }

    // -------------------------------------------------------------------------
    // shieldedTransfer: private value-preserving move. The action verified the
    // inputs exist and are unspent, that sum(inputs) == sum(outputs), and ran
    // OFAC on the recipients before signing. The chain sees only new
    // commitments, nullifiers, and encrypted blobs — no amounts, no parties.
    // -------------------------------------------------------------------------
    function shieldedTransfer(
        bytes32[] calldata inputNullifiers,
        bytes32[] calldata outputCommitments,
        string[] calldata encryptedBlobs,
        bytes32 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external {
        if (outputCommitments.length != encryptedBlobs.length) revert LengthMismatch();
        _checkAuth(
            keccak256(
                abi.encode(
                    "TRANSFER",
                    inputNullifiers,
                    outputCommitments,
                    encryptedBlobs,
                    nonce,
                    deadline,
                    address(this),
                    block.chainid
                )
            ),
            nonce,
            deadline,
            signature
        );

        _spend(inputNullifiers);
        _addNotes(outputCommitments, encryptedBlobs);
        // No totalSupply change: value is conserved.
    }

    // -------------------------------------------------------------------------
    // redeem: privUSD -> USDC. The action nullified enough input notes to cover
    // withdrawAmount and minted a change note for the remainder.
    // -------------------------------------------------------------------------
    function redeem(
        bytes32[] calldata inputNullifiers,
        bytes32[] calldata changeCommitments,
        string[] calldata changeBlobs,
        uint256 withdrawAmount,
        address recipient,
        bytes32 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external {
        if (changeCommitments.length != changeBlobs.length) revert LengthMismatch();
        if (withdrawAmount > totalSupply) revert InsufficientSupply();
        _checkAuth(
            keccak256(
                abi.encode(
                    "REDEEM",
                    inputNullifiers,
                    changeCommitments,
                    changeBlobs,
                    withdrawAmount,
                    recipient,
                    nonce,
                    deadline,
                    address(this),
                    block.chainid
                )
            ),
            nonce,
            deadline,
            signature
        );

        _spend(inputNullifiers);
        _addNotes(changeCommitments, changeBlobs);
        totalSupply -= withdrawAmount;
        reserve.safeTransfer(recipient, withdrawAmount);
        emit Redeemed(recipient, withdrawAmount);
    }

    // -------------------------------------------------------------------------
    // Internals.
    // -------------------------------------------------------------------------
    function _checkAuth(
        bytes32 structHash,
        bytes32 nonce,
        uint256 deadline,
        bytes calldata signature
    ) internal {
        if (block.timestamp > deadline) revert AuthorizationExpired();
        if (usedNonces[nonce]) revert NonceAlreadyUsed();
        address signer = structHash.toEthSignedMessageHash().recover(signature);
        if (signer != ledgerOracle) revert InvalidOracleSignature();
        usedNonces[nonce] = true;
    }

    function _spend(bytes32[] calldata ns) internal {
        for (uint256 i = 0; i < ns.length; i++) {
            if (nullifiers[ns[i]]) revert NoteAlreadySpent();
            nullifiers[ns[i]] = true;
            emit NoteSpent(ns[i]);
        }
    }

    function _addNotes(bytes32[] calldata cs, string[] calldata blobs) internal {
        for (uint256 i = 0; i < cs.length; i++) {
            if (commitments[cs[i]]) revert CommitmentExists();
            commitments[cs[i]] = true;
            emit NoteCreated(cs[i], blobs[i]);
        }
    }
}
