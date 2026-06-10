// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {Ownable, Ownable2Step} from "@openzeppelin/contracts/access/Ownable2Step.sol";

/// @title BridgeToken
/// @notice A burn/mint cross-chain ERC-20. The same contract is deployed on
///         every chain the token should live on. To move tokens from chain A
///         to chain B, the holder calls `burn` on A (destroying local supply
///         and emitting `BurnInitiated`); the lit-bridge oracle observes the
///         burn across N RPCs, signs a mint authorization, and anyone can call
///         `mint` on B with that signature.
///
/// Trust (see plans/hyperlane-competitor.md):
///   * `bridgeOracle` is the address of the bridge's dedicated signing account
///     (Option B). Unlike a CID-derived signer, this address is **stable across
///     action logic upgrades** — fixing a bug in the verification action does
///     not rotate the oracle, so no BridgeToken ever needs to be re-pointed.
///     The account is governed by the Base Safe.
///   * `bridgePartner[srcChainId] => address` pins the trusted sibling contract
///     per source chain. Wiring is **write-once per chain**: once set non-zero
///     it can never change, closing the "owner re-points partner at a copycat
///     and mints unbacked supply" attack.
///   * Each `(srcChainId, burnTxHash, logIndex)` triple can mint exactly once —
///     `usedBurnIds` prevents replay.
///
/// Note: this contract is intentionally unchanged from the original example. It
/// only ever sees an immutable oracle *address* and does not care how that key
/// is custodied — the registry / consensus / encryption work all lives in the
/// off-chain action and the BridgeConfigRegistry.
contract BridgeToken is ERC20, Ownable2Step {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    /// @notice Account authorized to attest cross-chain burns (the bridge
    ///         signing account, governed by the Base Safe).
    address public immutable bridgeOracle;

    /// @notice `owner` (from Ownable2Step) wires sibling chains + sets fee/gas
    ///         config. Two-step transfer hands control to the Base Safe (Phase 6).

    /// @notice Trusted BridgeToken deployment per source chain id.
    mapping(uint256 => address) public bridgePartner;

    /// @notice Burns already redeemed here — keyed on
    ///         keccak256(srcChainId, burnTxHash, logIndex).
    mapping(bytes32 => bool) public usedBurnIds;

    /// @notice Chain-local counter mixed into `BurnInitiated`.
    uint256 public burnNonce;

    /// @notice Fee config (set by owner). On mint, `feeFlat + amount*feeBps/1e4`
    ///         is minted to `feeTreasury` and the remainder to the recipient.
    ///         No fee is taken while `feeTreasury` is the zero address. The skim
    ///         accrues the bridged token in the treasury; converting it to gas
    ///         to refill the oracle is an off-chain concern (deferred).
    address public feeTreasury;
    uint256 public feeFlat;
    uint16 public feeBps;
    uint16 public constant MAX_FEE_BPS = 500; // 5% hard cap, enforced on set

    error FeeBpsTooHigh();
    error UnknownSourceChain();
    error UnknownDestinationChain();
    error SourceContractMismatch();
    error AlreadyRedeemed();
    error AuthorizationExpired();
    error InvalidBridgeSignature();
    error PartnerAlreadySet();
    error ZeroRecipient();
    error ZeroPartner();
    error SweepFailed();

    event BurnInitiated(
        address indexed from,
        address indexed recipient,
        uint256 amount,
        uint256 indexed destChainId,
        uint256 nonce,
        uint256 gasPrepaid
    );

    event GasSwept(address indexed to, uint256 amount);

    event BridgeMint(
        address indexed recipient,
        uint256 amount, // net amount minted to recipient (after fee)
        uint256 fee, // skimmed to feeTreasury
        uint256 indexed srcChainId,
        bytes32 burnTxHash,
        uint256 logIndex
    );

    event BridgePartnerSet(uint256 indexed chainId, address partner);
    event FeeConfigSet(address treasury, uint256 feeFlat, uint16 feeBps);

    constructor(
        string memory name_,
        string memory symbol_,
        uint256 initialSupply,
        address oracle_
    ) ERC20(name_, symbol_) Ownable(msg.sender) {
        bridgeOracle = oracle_;
        if (initialSupply > 0) {
            _mint(msg.sender, initialSupply);
        }
    }

    /// @notice Burn `amount` from the caller and announce intent to mint on
    ///         `destChainId` at `recipient`.
    function burn(uint256 amount, uint256 destChainId, address recipient)
        external
        payable
        returns (uint256)
    {
        if (bridgePartner[destChainId] == address(0)) revert UnknownDestinationChain();
        if (recipient == address(0)) revert ZeroRecipient();
        uint256 n = ++burnNonce;
        _burn(msg.sender, amount);
        // `msg.value` is an optional native gas prepay to reimburse the relayer
        // for the destination mint. It pools in this contract for the owner to
        // sweep. The relayer auto-mints ONLY when the prepay covers destination
        // gas — so an un(der)-prepaid burn simply isn't auto-relayed (the holder
        // can still self-submit `mint` and pay their own gas). This is what makes
        // bridging an illiquid token safe: no prepay, no relayer gas spent.
        emit BurnInitiated(msg.sender, recipient, amount, destChainId, n, msg.value);
        return n;
    }

    /// @notice Withdraw pooled gas prepays (native) to `to`. Owner-only. The
    ///         owner moves these to the relayer's gas wallet (cross-chain
    ///         rebalancing is off-chain).
    function sweepGas(address to) external onlyOwner {
        uint256 bal = address(this).balance;
        (bool ok,) = to.call{value: bal}("");
        if (!ok) revert SweepFailed();
        emit GasSwept(to, bal);
    }

    /// @notice Redeem a burn observed on another chain. Anyone can submit — the
    ///         signature authorizes the mint, not the caller.
    function mint(
        uint256 srcChainId,
        address srcContract,
        bytes32 burnTxHash,
        uint256 logIndex,
        address recipient,
        uint256 amount,
        uint256 srcNonce,
        uint256 deadline,
        bytes calldata signature
    ) external {
        if (block.timestamp > deadline) revert AuthorizationExpired();

        address expected = bridgePartner[srcChainId];
        if (expected == address(0)) revert UnknownSourceChain();
        if (expected != srcContract) revert SourceContractMismatch();

        bytes32 burnId = keccak256(abi.encode(srcChainId, burnTxHash, logIndex));
        if (usedBurnIds[burnId]) revert AlreadyRedeemed();

        bytes32 digest = keccak256(
            abi.encode(
                srcChainId,
                srcContract,
                burnTxHash,
                logIndex,
                recipient,
                amount,
                srcNonce,
                deadline,
                address(this),
                block.chainid
            )
        ).toEthSignedMessageHash();

        if (digest.recover(signature) != bridgeOracle) {
            revert InvalidBridgeSignature();
        }

        usedBurnIds[burnId] = true;

        // Skim the fee to the treasury, mint the remainder to the recipient.
        // The fee is clamped to `amount` so the destination mint can NEVER
        // revert on fee config — the source tokens are already burned, so a
        // revert here would strand them.
        uint256 fee;
        address treasury = feeTreasury;
        if (treasury != address(0)) {
            fee = feeFlat + (amount * feeBps) / 10_000;
            if (fee > amount) fee = amount;
            if (fee > 0) _mint(treasury, fee);
        }
        uint256 net = amount - fee;
        _mint(recipient, net);
        emit BridgeMint(recipient, net, fee, srcChainId, burnTxHash, logIndex);
    }

    /// @notice Set the cross-chain mint fee. Owner-only, capped at MAX_FEE_BPS.
    ///         Set `treasury` to the zero address to disable fees.
    function setFeeConfig(address treasury, uint256 feeFlat_, uint16 feeBps_) external onlyOwner {
        if (feeBps_ > MAX_FEE_BPS) revert FeeBpsTooHigh();
        feeTreasury = treasury;
        feeFlat = feeFlat_;
        feeBps = feeBps_;
        emit FeeConfigSet(treasury, feeFlat_, feeBps_);
    }

    /// @notice One-time wiring of a sibling chain. Write-once per `chainId`.
    function setBridgePartner(uint256 chainId, address partner) external onlyOwner {
        if (partner == address(0)) revert ZeroPartner();
        if (bridgePartner[chainId] != address(0)) revert PartnerAlreadySet();
        bridgePartner[chainId] = partner;
        emit BridgePartnerSet(chainId, partner);
    }
}
