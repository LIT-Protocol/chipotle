// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @title SolverVault
/// @notice Inventory custody for an intent-system solver/filler. Funds leave
///         the vault on exactly two paths:
///
///         1. `executeFill` — requires a fresh signature from the `policySigner`,
///            an identity derived from a Lit Action's IPFS CID. That action is
///            the policy: it screens the fill (recipient binding, notional cap,
///            settlement allowlist, kill switch) and only signs when it passes.
///            The solver bot never holds a key that can move inventory — it can
///            only *ask* Lit to authorize a fill.
///
///         2. `exit` — the local `owner` break-glass key sweeps the full balance
///            to a pinned `coldWallet`. Always works, needs no Lit. This is the
///            liveness guarantee: a Lit outage stops new fills, it never traps
///            inventory.
///
///         The only thing protecting funds from a *combined* compromise is the
///         exit destination, so changing `coldWallet` is the protected, slow
///         path (a 7-day timelock). Exits are fast; destination changes are slow.
///
/// @dev Trust model recap:
///       - solver bot      → holds only a Lit usage key; can request fills.
///       - policySigner    → the Lit Action; authorizes good fills, refuses bad.
///       - owner           → local key (recommend a Safe). Restricts policy +
///                           exits to coldWallet. Cannot redirect a fill.
///       - coldWallet      → pinned safe destination (recommend a Safe).
contract SolverVault {
    using SafeERC20 for IERC20;
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    /// @notice Action-derived signer that authorizes fills. Bound to the policy
    ///         action's IPFS CID — edit the policy and this address changes, so
    ///         the vault stops honoring the modified action automatically.
    address public immutable policySigner;

    /// @notice Local break-glass key. Restricts policy and sweeps to coldWallet.
    address public owner;

    /// @notice Pinned destination for emergency exits. Recommend a Safe multisig.
    address public coldWallet;

    // --- on-chain policy config (read by the policy action before it signs) ---

    /// @notice Global stop. When true the action refuses to sign any fill.
    bool public killSwitch;

    /// @notice Per-fill cap in raw token units. The action refuses fills above it.
    uint256 public maxFillAmount;

    /// @notice Settlement contracts this vault fills for. The action refuses
    ///         fills referencing a settlement contract not in this set.
    mapping(address => bool) public allowedSettlement;

    /// @notice Replay protection: each fill nonce may be spent once.
    mapping(bytes32 => bool) public usedNonces;

    /// @notice Longest a policy signature may stay valid. Bounds the window in
    ///         which a pre-minted authorization can be replayed after policy
    ///         tightens (kill switch / lower cap).
    uint256 public constant MAX_AUTH_TTL = 1 hours;

    // --- cold-wallet change timelock ---

    uint256 public constant COLD_WALLET_TIMELOCK = 7 days;
    address public pendingColdWallet;
    uint256 public pendingColdWalletReadyAt;

    error NotOwner();
    error FillExpired();
    error DeadlineTooFar();
    error KillSwitchEngaged();
    error OverCap();
    error NonceAlreadyUsed();
    error InvalidPolicySignature();
    error NoPendingColdWalletChange();
    error TimelockNotElapsed();
    error ZeroAddress();

    event FillExecuted(
        address indexed token,
        address indexed recipient,
        uint256 amount,
        bytes32 indexed nonce
    );
    event Exited(address indexed token, address indexed coldWallet, uint256 amount);
    event KillSwitchSet(bool on);
    event MaxFillAmountSet(uint256 amount);
    event AllowedSettlementSet(address indexed settlement, bool allowed);
    event ColdWalletChangeRequested(address indexed newColdWallet, uint256 readyAt);
    event ColdWalletChanged(address indexed newColdWallet);
    event OwnerTransferred(address indexed newOwner);

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    constructor(
        address policySigner_,
        address owner_,
        address coldWallet_,
        uint256 maxFillAmount_
    ) {
        if (policySigner_ == address(0) || owner_ == address(0) || coldWallet_ == address(0)) {
            revert ZeroAddress();
        }
        policySigner = policySigner_;
        owner = owner_;
        coldWallet = coldWallet_;
        maxFillAmount = maxFillAmount_;
    }

    // -------------------------------------------------------------------------
    // Fills
    // -------------------------------------------------------------------------

    /// @notice Release `amount` of `token` to `recipient` against a policy
    ///         signature. The signature is over the EIP-191 hash of
    ///         (token, recipient, amount, nonce, deadline, this vault, chainid)
    ///         — the exact tuple the Lit Action signs after passing policy.
    /// @dev    The on-chain check here is intentionally minimal: it only proves
    ///         "the policy action authorized exactly this fill." All the policy
    ///         logic (recipient binding to the on-chain order, caps, allowlist,
    ///         kill switch) ran off-chain inside the action before it produced
    ///         this signature. A bad fill never gets a signature in the first
    ///         place, so it never reaches this function.
    function executeFill(
        address token,
        address recipient,
        uint256 amount,
        bytes32 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external {
        if (block.timestamp > deadline) revert FillExpired();
        if (deadline > block.timestamp + MAX_AUTH_TTL) revert DeadlineTooFar();
        if (usedNonces[nonce]) revert NonceAlreadyUsed();
        // Re-enforce the time-sensitive policy on-chain. The action checks
        // these at signing time, but a signature minted while policy was
        // permissive must not execute after the owner engages the kill switch
        // or lowers the cap.
        if (killSwitch) revert KillSwitchEngaged();
        if (amount > maxFillAmount) revert OverCap();

        bytes32 digest = keccak256(
            abi.encode(
                token,
                recipient,
                amount,
                nonce,
                deadline,
                address(this),
                block.chainid
            )
        ).toEthSignedMessageHash();

        if (digest.recover(signature) != policySigner) revert InvalidPolicySignature();

        usedNonces[nonce] = true;
        IERC20(token).safeTransfer(recipient, amount);

        emit FillExecuted(token, recipient, amount, nonce);
    }

    // -------------------------------------------------------------------------
    // Policy config (owner)
    // -------------------------------------------------------------------------
    //
    // For this example the policy config lives on-chain so the demo can update
    // it with a single transaction and the action reads it with a plain
    // eth_call. In production you'd likely keep this config in a signed
    // off-chain blob instead — public on-chain caps/allowlists leak a solver's
    // strategy to competitors (see README "Production hardening").

    function setKillSwitch(bool on) external onlyOwner {
        killSwitch = on;
        emit KillSwitchSet(on);
    }

    function setMaxFillAmount(uint256 amount) external onlyOwner {
        maxFillAmount = amount;
        emit MaxFillAmountSet(amount);
    }

    function setAllowedSettlement(address settlement, bool allowed) external onlyOwner {
        allowedSettlement[settlement] = allowed;
        emit AllowedSettlementSet(settlement, allowed);
    }

    // -------------------------------------------------------------------------
    // Emergency exit (owner) — always available, no Lit dependency
    // -------------------------------------------------------------------------

    /// @notice Sweep the vault's entire `token` balance to `coldWallet`.
    ///         Callable any time by the owner regardless of Lit's state. The
    ///         destination is pinned, so even a compromised owner key can only
    ///         push funds where you already approved them to go.
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
