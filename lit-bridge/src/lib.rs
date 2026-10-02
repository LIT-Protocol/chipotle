//! lit-bridge — permissionless cross-chain token bridge service.
//!
//! See `plans/hyperlane-competitor.md` for the full design. This crate is the
//! hosted product surface: it serves the bridging web UI and a small config
//! endpoint the UI bootstraps from. It is **stateless** — all bridge state
//! lives on-chain (the registry, the burn/mint events, `usedBurnIds`), so there
//! is no database. The trust-layer logic lives in `action/bridgeAction.js`; the
//! on-chain control plane lives in `contracts/`.

pub mod config;
