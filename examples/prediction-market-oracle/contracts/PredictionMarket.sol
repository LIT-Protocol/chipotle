// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @title PredictionMarket
/// @notice Minimal question-and-resolution registry. Anyone can `propose` a
///         yes/no question with a `resolveAt` timestamp. After that time,
///         anyone can `resolve` it by supplying a signature from the
///         configured `oracle` address — which in this example is derived
///         from the Lit Action's IPFS CID, so the contract trusts "this
///         exact AI-consensus action code" rather than a generic signer.
///
/// This contract intentionally has no betting / stake / payout logic — a
/// real prediction market would consume the resolution from here. The
/// example is about the Lit-shaped piece: using a multi-model AI consensus
/// to attest an answer on-chain.
contract PredictionMarket {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    enum Answer { Unresolved, Yes, No, Unclear }

    struct Question {
        string text;
        uint256 resolveAt;
        address proposer;
        Answer answer;
        uint64 resolvedAt;
    }

    /// @notice Address authorized to sign resolutions. In this example,
    ///         derived from the Lit Action's IPFS CID.
    address public immutable oracle;

    /// @dev keyed on keccak256(bytes(text))
    mapping(bytes32 => Question) public questions;

    error AlreadyProposed();
    error NotYetResolvable();
    error AlreadyResolved();
    error UnknownAnswer();
    error Expired();
    error InvalidSignature();
    error EmptyText();

    event QuestionProposed(bytes32 indexed id, address indexed proposer, string text, uint256 resolveAt);
    event QuestionResolved(bytes32 indexed id, Answer answer, uint64 resolvedAt);

    constructor(address oracle_) {
        oracle = oracle_;
    }

    function questionId(string calldata text) public pure returns (bytes32) {
        return keccak256(bytes(text));
    }

    function propose(string calldata text, uint256 resolveAt) external returns (bytes32 id) {
        if (bytes(text).length == 0) revert EmptyText();
        id = keccak256(bytes(text));
        if (questions[id].resolveAt != 0) revert AlreadyProposed();
        questions[id] = Question({
            text: text,
            resolveAt: resolveAt,
            proposer: msg.sender,
            answer: Answer.Unresolved,
            resolvedAt: 0
        });
        emit QuestionProposed(id, msg.sender, text, resolveAt);
    }

    function resolve(
        bytes32 id,
        uint8 answer,
        uint256 deadline,
        bytes calldata signature
    ) external {
        Question storage q = questions[id];
        if (q.resolveAt == 0) revert AlreadyResolved(); // not proposed
        if (q.answer != Answer.Unresolved) revert AlreadyResolved();
        if (block.timestamp < q.resolveAt) revert NotYetResolvable();
        if (block.timestamp > deadline) revert Expired();
        if (answer == 0 || answer > 3) revert UnknownAnswer();

        bytes32 digest = keccak256(
            abi.encode(address(this), id, answer, deadline, block.chainid)
        ).toEthSignedMessageHash();

        if (digest.recover(signature) != oracle) revert InvalidSignature();

        Answer a = Answer(answer);
        q.answer = a;
        q.resolvedAt = uint64(block.timestamp);

        emit QuestionResolved(id, a, uint64(block.timestamp));
    }
}
