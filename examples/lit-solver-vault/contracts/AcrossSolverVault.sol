// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @notice The subset of an Across V3 SpokePool this vault calls. `fillV3Relay`
///         pulls `outputAmount` of `outputToken` from `msg.sender` (this vault)
///         and sends it to `relayData.recipient`.
interface ISpokePool {
    struct V3RelayData {
        address depositor;
        address recipient;
        address exclusiveRelayer;
        address inputToken;
        address outputToken;
        uint256 inputAmount;
        uint256 outputAmount;
        uint256 originChainId;
        uint32 depositId;
        uint32 fillDeadline;
        uint32 exclusivityDeadline;
        bytes message;
    }

    function fillV3Relay(V3RelayData calldata relayData, uint256 repaymentChainId) external;
}

/// @title AcrossSolverVault
/// @notice The real-integration sibling of `SolverVault`: inventory custody for
///         an Across relayer (filler). Identical trust model — the bot holds
///         only a Lit usage key, the policy action authorizes fills, the owner
///         can only restrict policy and exit to a pinned cold wallet — but a
///         fill here executes against a live Across SpokePool instead of a bare
///         transfer.
///
///         Why the policy matters more here, not less: on the destination chain
///         `fillV3Relay` does NOT check the fill against the origin deposit —
///         Across only reconciles fills to deposits later, during reimbursement.
///         So nothing on-chain stops a compromised bot from filling to an
///         attacker and eating the loss. The Lit Action reading the real deposit
///         and binding the fill to it is exactly the protection that prevents
///         that. See action/acrossPolicy.js.
contract AcrossSolverVault {
    using SafeERC20 for IERC20;
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    /// @notice The Across SpokePool on this (destination) chain.
    address public immutable spokePool;

    /// @notice Action-derived signer. Bound to acrossPolicy.js's IPFS CID.
    address public immutable policySigner;

    /// @notice Local break-glass key. Restricts policy and sweeps to coldWallet.
    address public owner;

    /// @notice Pinned destination for emergency exits. Recommend a Safe multisig.
    address public coldWallet;

    // --- on-chain policy config (read by the policy action before it signs) ---
    bool public killSwitch;
    uint256 public maxFillAmount;
    /// @notice Origin chains (by chain id) this vault will fill deposits from.
    mapping(uint256 => bool) public allowedOriginChain;

    /// @notice Longest a policy signature may stay valid. Bounds the window in
    ///         which a pre-minted authorization can be replayed after policy
    ///         tightens (kill switch / lower cap).
    uint256 public constant MAX_AUTH_TTL = 1 hours;

    // --- cold-wallet change timelock ---
    uint256 public constant COLD_WALLET_TIMELOCK = 7 days;
    address public pendingColdWallet;
    uint256 public pendingColdWalletReadyAt;

    error NotOwner();
    error AuthExpired();
    error AuthDeadlineTooFar();
    error KillSwitchEngaged();
    error OverCap();
    error InvalidPolicySignature();
    error NoPendingColdWalletChange();
    error TimelockNotElapsed();
    error ZeroAddress();

    event AcrossFillExecuted(
        uint32 indexed depositId,
        uint256 indexed originChainId,
        address indexed recipient,
        address outputToken,
        uint256 outputAmount
    );
    event Exited(address indexed token, address indexed coldWallet, uint256 amount);
    event KillSwitchSet(bool on);
    event MaxFillAmountSet(uint256 amount);
    event AllowedOriginChainSet(uint256 indexed originChainId, bool allowed);
    event ColdWalletChangeRequested(address indexed newColdWallet, uint256 readyAt);
    event ColdWalletChanged(address indexed newColdWallet);
    event OwnerTransferred(address indexed newOwner);

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    constructor(
        address spokePool_,
        address policySigner_,
        address owner_,
        address coldWallet_,
        uint256 maxFillAmount_
    ) {
        if (
            spokePool_ == address(0) ||
            policySigner_ == address(0) ||
            owner_ == address(0) ||
            coldWallet_ == address(0)
        ) revert ZeroAddress();
        spokePool = spokePool_;
        policySigner = policySigner_;
        owner = owner_;
        coldWallet = coldWallet_;
        maxFillAmount = maxFillAmount_;
    }

    // -------------------------------------------------------------------------
    // Fills
    // -------------------------------------------------------------------------

    /// @notice Fill an Across deposit against a policy signature. The signature
    ///         is over the EIP-191 hash of
    ///         (relayData, repaymentChainId, authDeadline, this vault, chainid)
    ///         — committing to every field of the relay, so the bot can't alter
    ///         the recipient or amount after policy approved it.
    /// @dev    Replay is handled by the SpokePool itself (a deposit can only be
    ///         filled once; a second fill reverts), plus `authDeadline`.
    function executeAcrossFill(
        ISpokePool.V3RelayData calldata relayData,
        uint256 repaymentChainId,
        uint256 authDeadline,
        bytes calldata signature
    ) external {
        if (block.timestamp > authDeadline) revert AuthExpired();
        if (authDeadline > block.timestamp + MAX_AUTH_TTL) revert AuthDeadlineTooFar();
        // Re-enforce the time-sensitive policy on-chain so an authorization
        // minted while policy was permissive can't execute after the owner
        // engages the kill switch or lowers the cap.
        if (killSwitch) revert KillSwitchEngaged();
        if (relayData.outputAmount > maxFillAmount) revert OverCap();

        bytes32 digest = keccak256(
            abi.encode(relayData, repaymentChainId, authDeadline, address(this), block.chainid)
        ).toEthSignedMessageHash();
        if (digest.recover(signature) != policySigner) revert InvalidPolicySignature();

        // Approve exactly this fill's output, then fill. forceApprove resets any
        // stale allowance first (some tokens require allowance==0 before reset).
        IERC20(relayData.outputToken).forceApprove(spokePool, relayData.outputAmount);
        ISpokePool(spokePool).fillV3Relay(relayData, repaymentChainId);

        emit AcrossFillExecuted(
            relayData.depositId,
            relayData.originChainId,
            relayData.recipient,
            relayData.outputToken,
            relayData.outputAmount
        );
    }

    // -------------------------------------------------------------------------
    // Policy config (owner) — see SolverVault for the on-chain-config rationale.
    // -------------------------------------------------------------------------

    function setKillSwitch(bool on) external onlyOwner {
        killSwitch = on;
        emit KillSwitchSet(on);
    }

    function setMaxFillAmount(uint256 amount) external onlyOwner {
        maxFillAmount = amount;
        emit MaxFillAmountSet(amount);
    }

    function setAllowedOriginChain(uint256 originChainId, bool allowed) external onlyOwner {
        allowedOriginChain[originChainId] = allowed;
        emit AllowedOriginChainSet(originChainId, allowed);
    }

    // -------------------------------------------------------------------------
    // Emergency exit (owner) — always available, no Lit / no Across dependency
    // -------------------------------------------------------------------------

    function exit(address token) external onlyOwner {
        uint256 bal = IERC20(token).balanceOf(address(this));
        IERC20(token).safeTransfer(coldWallet, bal);
        emit Exited(token, coldWallet, bal);
    }

    // -------------------------------------------------------------------------
    // Cold-wallet change (owner) — the slow, protected path
    // -------------------------------------------------------------------------

    function requestColdWalletChange(address newColdWallet) external onlyOwner {
        if (newColdWallet == address(0)) revert ZeroAddress();
        pendingColdWallet = newColdWallet;
        pendingColdWalletReadyAt = block.timestamp + COLD_WALLET_TIMELOCK;
        emit ColdWalletChangeRequested(newColdWallet, pendingColdWalletReadyAt);
    }

    function commitColdWalletChange() external onlyOwner {
        if (pendingColdWallet == address(0)) revert NoPendingColdWalletChange();
        if (block.timestamp < pendingColdWalletReadyAt) revert TimelockNotElapsed();
        coldWallet = pendingColdWallet;
        pendingColdWallet = address(0);
        pendingColdWalletReadyAt = 0;
        emit ColdWalletChanged(coldWallet);
    }

    function transferOwnership(address newOwner) external onlyOwner {
        if (newOwner == address(0)) revert ZeroAddress();
        owner = newOwner;
        emit OwnerTransferred(newOwner);
    }
}
