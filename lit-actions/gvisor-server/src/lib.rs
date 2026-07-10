//! Any-language Lit Action runner.
//!
//! Serves the exact same `Action` gRPC op-loop as the JS runner
//! (`lit-actions/server`), but on its own Unix socket, and executes each
//! request inside a gVisor (`runsc`) sandbox instead of a Deno isolate. The
//! "code" is a content-addressed bundle (tarball with a `lit.json` manifest)
//! whose entrypoint may be written in any language; a preinstalled `lit` CLI
//! (see `src/bin/lit.rs`) exposes the op-loop ops to guest code over a
//! per-execution host-UDS socket.
//!
//! lit-api-server talks to this runner exactly the way it talks to the JS
//! runner — only the socket path differs.

pub mod bridge;
pub mod bundle;
mod guest_service;
pub mod proto;
pub mod sandbox;
pub mod server;
pub mod supervisor;

pub use server::{GvisorServer, start_server};
pub use supervisor::Supervisor;

// Re-export the op-loop protocol pieces so binaries/tests of this crate need
// no direct lit-actions-grpc dependency.
pub use lit_actions_grpc::{proto as oploop, unix};
