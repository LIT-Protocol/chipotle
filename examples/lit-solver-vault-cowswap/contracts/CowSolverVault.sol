// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @title CowSolverVault
/// @notice The CoW Protocol sibling of `SolverVault` / `AcrossSolverVault`:
///         inventory custody for a CoW solver. The vault is registered as an
///         allowlisted solver on a `GPv2Settlement`, so it is the only identity
///         that can call `settle`. It will only do so against a fresh signature
///         from `policySigner` — an identity derived from a Lit Action's IPFS
///         CID. That action is the policy: it validates the trader's signed
///         order, builds the canonical settlement, and signs it. The solver bot
///         never holds a key that can settle; it can only *ask* Lit to.
///
///         Why the policy matters here, not less: CoW settlement is permissioned
///         (only allowlisted solvers settle), but the allowlisted identity — the
///         settlement key on a normal solver's box — can submit *any* batch the
///         protocol accepts, including interactions that route the solver's own
///         inventory to an attacker. Here there is no settlement key on the box.
///         The Lit Action builds the settlement from the trader-signed order and
///         signs only that, so a compromised bot can't craft a self-dealing
///         batch. See action/cowPolicy.js.
///
/// @dev Trust model mirrors the family:
///       - solver bot   → holds only a Lit usage key; can request a settlement.
///       - policySigner → the Lit Action; authorizes good settlements only.
///       - owner        → local key (recommend a Safe). Restricts policy + exits
///                        to coldWallet. Cannot redirect a settlement.
///       - coldWallet   → pinned safe destination (recommend a Safe).
contract CowSolverVault {
    using SafeERC20 for IERC20;
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    /// @notice The GPv2Settlement this vault settles against. Pinned at deploy;
    ///         the policy action reads this and will only build settlements for
    ///         this exact contract — the trust anchor for the settle target and
    ///         the EIP-712 order domain.
    address public immutable settlement;

    /// @notice Action-derived signer. Bound to cowPolicy.js's IPFS CID — edit
    ///         the policy by a byte and this address changes, so the vault stops
    ///         honoring the modified action automatically.
    address public immutable policySigner;

    /// @notice Local break-glass key. Restricts policy and sweeps to coldWallet.
    address public owner;

    /// @notice Pinned destination for emergency exits. Recommend a Safe multisig.
    address public coldWallet;

    // --- on-chain policy config (read by the policy action before it signs) ---

    /// @notice Global stop. When true the action refuses to authorize, and
    ///         `executeSettlement` refuses to run, any settlement.
    bool public killSwitch;

    /// @notice Per-settlement cap on the inventory this vault will spend, in raw
    ///         units of the pulled (buy) token. Re-enforced on-chain.
    uint256 public maxFillAmount;

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
    error SettlementCallFailed();
    error NoPendingColdWalletChange();
    error TimelockNotElapsed();
    error ZeroAddress();

    event SettlementExecuted(bytes32 indexed calldataHash, address pullToken, uint256 pullAmount);
    event Exited(address indexed token, address indexed coldWallet, uint256 amount);
    event KillSwitchSet(bool on);
    event MaxFillAmountSet(uint256 amount);
    event ColdWalletChangeRequested(address indexed newColdWallet, uint256 readyAt);
    event ColdWalletChanged(address indexed newColdWallet);
    event OwnerTransferred(address indexed newOwner);

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    constructor(
        address settlement_,
        address policySigner_,
        address owner_,
        address coldWallet_,
        uint256 maxFillAmount_
    ) {
        if (
            settlement_ == address(0) ||
            policySigner_ == address(0) ||
            owner_ == address(0) ||
            coldWallet_ == address(0)
        ) revert ZeroAddress();
        settlement = settlement_;
        policySigner = policySigner_;
        owner = owner_;
        coldWallet = coldWallet_;
        maxFillAmount = maxFillAmount_;
    }

    // -------------------------------------------------------------------------
    // Settlement
    // -------------------------------------------------------------------------

    /// @notice Execute a CoW settlement against a policy signature.
    ///
    ///         The policy action returns the exact ABI-encoded `settle(...)`
    ///         calldata it built from the trader's signed order, plus the
    ///         (token, amount) of inventory the batch spends. The signature is
    ///         over the EIP-191 hash of
    ///         (keccak256(settleCalldata), pullToken, pullAmount, authDeadline,
    ///          this vault, chainid)
    ///         — committing to the *entire* batch. The bot can't alter a byte of
    ///         the settlement after policy approved it, and can't make the vault
    ///         spend a different token or more than `pullAmount`.
    ///
    /// @dev    The vault is the allowlisted solver, so this forwards the raw
    ///         calldata to the pinned settlement. Inventory leaves only through
    ///         a bounded approval: the vault approves the settlement for exactly
    ///         `pullAmount` of `pullToken` (the batch's pre-interaction does the
    ///         `transferFrom`), then resets the allowance to zero. So even the
    ///         settlement contract can never pull more inventory than this one
    ///         batch needs.
    function executeSettlement(
        bytes calldata settleCalldata,
        address pullToken,
        uint256 pullAmount,
        uint256 authDeadline,
        bytes calldata signature
    ) external {
        if (block.timestamp > authDeadline) revert AuthExpired();
        if (authDeadline > block.timestamp + MAX_AUTH_TTL) revert AuthDeadlineTooFar();
        // Re-enforce time-sensitive policy on-chain: a signature minted while
        // policy was permissive must not execute after the owner engages the
        // kill switch or lowers the cap.
        if (killSwitch) revert KillSwitchEngaged();
        if (pullAmount > maxFillAmount) revert OverCap();

        bytes32 digest = keccak256(
            abi.encode(
                keccak256(settleCalldata),
                pullToken,
                pullAmount,
                authDeadline,
                address(this),
                block.chainid
            )
        ).toEthSignedMessageHash();
        if (digest.recover(signature) != policySigner) revert InvalidPolicySignature();

        // Bounded inventory exposure: approve exactly this batch's spend, then
        // forward the settle calldata, then revoke. forceApprove resets any
        // stale allowance first (some tokens require allowance==0 before reset).
        IERC20(pullToken).forceApprove(settlement, pullAmount);
        (bool ok, bytes memory ret) = settlement.call(settleCalldata);
        IERC20(pullToken).forceApprove(settlement, 0);
        if (!ok) {
            // Bubble up the settlement's revert reason.
            if (ret.length > 0) {
                assembly {
                    revert(add(ret, 0x20), mload(ret))
                }
            }
            revert SettlementCallFailed();
        }

        emit SettlementExecuted(keccak256(settleCalldata), pullToken, pullAmount);
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

    // -------------------------------------------------------------------------
    // Emergency exit (owner) — always available, no Lit / no CoW dependency
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
