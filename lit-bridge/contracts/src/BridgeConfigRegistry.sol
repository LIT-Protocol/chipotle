// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Ownable2Step, Ownable} from "@openzeppelin/contracts/access/Ownable2Step.sol";

/// @title BridgeConfigRegistry
/// @notice Control-plane configuration for the lit-bridge oracle. Lives on one
///         chain (Base) and is read at runtime by the bridge Lit Action to
///         decide, per source chain, which RPCs to query and how many must
///         agree (quorum) before a burn is considered real.
///
/// Why this exists (see plans/hyperlane-competitor.md):
///   * Moving per-chain RPC config out of the action source and onto this
///     contract means **adding a chain is a governed config write, not a code
///     edit** — the action's IPFS CID (and therefore the oracle's signer
///     address) stays stable, so no BridgeToken ever needs to be redeployed.
///   * The action pins only this registry's address + the host allowlist used
///     to read it; everything else is governed here.
///
/// Trust:
///   * `owner` is the Base Safe. Only it can change config. Every change emits
///     an event so watchers can react. This contract is therefore a load-
///     bearing part of the trust model — see the threat table in the plan.
///   * Secrets (RPC API keys / full URLs) are NEVER stored in plaintext. For
///     `Alchemy`/`Infura` entries only an encrypted API key is stored and the
///     action constructs the hostname from a code-resident map (config cannot
///     redirect those reads). For `Custom` entries a plaintext `host` plus an
///     encrypted full URL are stored; the action asserts they agree after
///     decrypting in-TEE.
contract BridgeConfigRegistry is Ownable2Step {
    enum RpcType {
        Alchemy, // 0 — encSecret = encrypted API key; host ignored
        Infura, //  1 — encSecret = encrypted API key; host ignored
        Custom //   2 — encSecret = encrypted full URL; host = expected hostname
    }

    struct RpcEntry {
        RpcType rpcType;
        string host; // plaintext expected hostname (Custom only; "" otherwise)
        string encSecret; // Lit-encrypted API key (Alchemy/Infura) or URL (Custom)
    }

    struct ChainConfig {
        bool exists;
        uint64 minConfirmations; // action floors this; 0 is allowed here
        uint8 quorum; // how many RPCs must agree; >= 1
        RpcEntry[] rpcs;
    }

    mapping(uint256 => ChainConfig) private _chains;

    error ZeroQuorum();
    error QuorumExceedsRpcCount();
    error CustomHostRequired();
    error EmptySecret();
    error ChainNotConfigured();
    error RpcIndexOutOfRange();

    event ChainConfigured(
        uint256 indexed chainId,
        uint64 minConfirmations,
        uint8 quorum,
        uint256 rpcCount
    );
    event ChainRemoved(uint256 indexed chainId);

    constructor(address initialOwner) Ownable(initialOwner) {}

    /// @notice Create or replace the full config for `chainId`. Wholesale
    ///         replace keeps reads simple and changes atomic. Owner-only.
    function setChain(
        uint256 chainId,
        uint64 minConfirmations,
        uint8 quorum,
        RpcEntry[] calldata rpcs
    ) external onlyOwner {
        if (quorum == 0) revert ZeroQuorum();
        if (rpcs.length < quorum) revert QuorumExceedsRpcCount();
        for (uint256 i = 0; i < rpcs.length; i++) {
            if (bytes(rpcs[i].encSecret).length == 0) revert EmptySecret();
            if (rpcs[i].rpcType == RpcType.Custom && bytes(rpcs[i].host).length == 0) {
                revert CustomHostRequired();
            }
        }

        ChainConfig storage c = _chains[chainId];
        c.exists = true;
        c.minConfirmations = minConfirmations;
        c.quorum = quorum;
        delete c.rpcs;
        for (uint256 i = 0; i < rpcs.length; i++) {
            c.rpcs.push(rpcs[i]);
        }

        emit ChainConfigured(chainId, minConfirmations, quorum, rpcs.length);
    }

    /// @notice Remove a chain entirely. Owner-only.
    function removeChain(uint256 chainId) external onlyOwner {
        if (!_chains[chainId].exists) revert ChainNotConfigured();
        delete _chains[chainId];
        emit ChainRemoved(chainId);
    }

    // ── Views read by the Lit Action (decoded via ethers.Interface) ─────────

    /// @notice Header for a chain. `exists` is false for unconfigured chains —
    ///         the action fails closed on that. Split from the RPC list so the
    ///         action can size its loop without returning unbounded arrays.
    function getChain(uint256 chainId)
        external
        view
        returns (bool exists, uint64 minConfirmations, uint8 quorum, uint256 rpcCount)
    {
        ChainConfig storage c = _chains[chainId];
        return (c.exists, c.minConfirmations, c.quorum, c.rpcs.length);
    }

    /// @notice One RPC entry by index. The action loops `0..rpcCount`.
    function getRpc(uint256 chainId, uint256 index)
        external
        view
        returns (uint8 rpcType, string memory host, string memory encSecret)
    {
        ChainConfig storage c = _chains[chainId];
        if (!c.exists) revert ChainNotConfigured();
        if (index >= c.rpcs.length) revert RpcIndexOutOfRange();
        RpcEntry storage e = c.rpcs[index];
        return (uint8(e.rpcType), e.host, e.encSecret);
    }
}
