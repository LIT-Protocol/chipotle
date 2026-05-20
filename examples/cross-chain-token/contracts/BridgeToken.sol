// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @title BridgeToken
/// @notice A burn/mint cross-chain ERC-20. The same contract is deployed on
///         every chain you want the token to live on. To move tokens from
///         chain A to chain B, the holder calls `burn` on A, which destroys
///         the supply locally and emits a `BurnInitiated` event carrying the
///         destination chain id, recipient, and a per-source nonce. Off-chain,
///         a Lit Action observes the burn (via `eth_getTransactionReceipt`),
///         signs a mint authorization, and anyone can call `mint` on B with
///         that signature to materialize the tokens on the destination chain.
///
/// Trust:
///   * Every deployment pins the same `bridgeOracle` address — the wallet
///     derived from the Lit Action's IPFS CID. Only signatures from that key
///     authorize a mint. Editing the action by a byte changes the CID, which
///     changes the signer, which makes every deployed contract refuse the
///     modified action's signatures.
///   * Each deployment also stores `bridgePartner[srcChainId] => address`
///     entries pointing at its sibling contracts on other chains, set by the
///     deployer via `setBridgePartner` during setup. Wiring is **write-once
///     per chain**: a partner can only be set if it's currently zero. This
///     closes the "owner re-points partner at a copycat source and mints
///     unbacked supply" attack — once wired, the deployer key has no
///     remaining privilege over an already-bridged chain.
///   * Each `(srcChainId, burnTxHash, logIndex)` triple can mint exactly
///     once — `usedBurnIds` prevents replay across calls.
contract BridgeToken is ERC20 {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    /// @notice Action-derived wallet authorized to attest cross-chain burns.
    address public immutable bridgeOracle;

    /// @notice Deployer-controlled admin used only to wire up sibling chain
    ///         addresses after both deployments exist (chicken-and-egg).
    address public owner;

    /// @notice Trusted BridgeToken deployment per source chain id. A burn
    ///         observed on a source chain only mints here if it originated
    ///         at this address.
    mapping(uint256 => address) public bridgePartner;

    /// @notice Burns that have already been redeemed on this chain — keyed
    ///         on keccak256(srcChainId, burnTxHash, logIndex).
    mapping(bytes32 => bool) public usedBurnIds;

    /// @notice Chain-local counter mixed into `BurnInitiated` so observers
    ///         have a stable per-chain ordering even before the tx is mined.
    uint256 public burnNonce;

    error NotOwner();
    error UnknownSourceChain();
    error UnknownDestinationChain();
    error SourceContractMismatch();
    error AlreadyRedeemed();
    error AuthorizationExpired();
    error InvalidBridgeSignature();
    error PartnerAlreadySet();
    error ZeroRecipient();
    error ZeroPartner();

    event BurnInitiated(
        address indexed from,
        address indexed recipient,
        uint256 amount,
        uint256 indexed destChainId,
        uint256 nonce
    );

    event BridgeMint(
        address indexed recipient,
        uint256 amount,
        uint256 indexed srcChainId,
        bytes32 burnTxHash,
        uint256 logIndex
    );

    event BridgePartnerSet(uint256 indexed chainId, address partner);

    constructor(
        string memory name_,
        string memory symbol_,
        uint256 initialSupply,
        address oracle_
    ) ERC20(name_, symbol_) {
        bridgeOracle = oracle_;
        owner = msg.sender;
        if (initialSupply > 0) {
            _mint(msg.sender, initialSupply);
        }
    }

    /// @notice Burn `amount` tokens from the caller's balance and announce
    ///         an intent to mint them on `destChainId` at `recipient`.
    ///         The off-chain Lit Action picks this event up from
    ///         `eth_getTransactionReceipt` and signs a corresponding mint.
    function burn(uint256 amount, uint256 destChainId, address recipient)
        external
        returns (uint256)
    {
        // Reject before burning so a typo or unwired chain can't permanently
        // destroy a holder's balance — the destination side would have no
        // way to honour the mint.
        if (bridgePartner[destChainId] == address(0)) revert UnknownDestinationChain();
        // _mint on the destination would revert for address(0), so the
        // tokens would be permanently lost. Catch it here.
        if (recipient == address(0)) revert ZeroRecipient();
        uint256 n = ++burnNonce;
        _burn(msg.sender, amount);
        emit BurnInitiated(msg.sender, recipient, amount, destChainId, n);
        return n;
    }

    /// @notice Redeem a burn observed on another chain. Anyone can submit —
    ///         the signature is what authorizes the mint, not the caller.
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
        _mint(recipient, amount);
        emit BridgeMint(recipient, amount, srcChainId, burnTxHash, logIndex);
    }

    /// @notice One-time wiring of a sibling chain after both BridgeToken
    ///         deployments exist. Write-once per `chainId`: once a partner is
    ///         set non-zero, this function reverts for that chain forever.
    ///         If you typo a partner address during setup the only path
    ///         forward is to redeploy this contract — which is the same
    ///         path the example's "re-run setup is a fresh setup" model
    ///         already takes.
    function setBridgePartner(uint256 chainId, address partner) external {
        if (msg.sender != owner) revert NotOwner();
        if (partner == address(0)) revert ZeroPartner();
        if (bridgePartner[chainId] != address(0)) revert PartnerAlreadySet();
        bridgePartner[chainId] = partner;
        emit BridgePartnerSet(chainId, partner);
    }
}
