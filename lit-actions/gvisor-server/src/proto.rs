//! Generated guest-ops gRPC bindings.
//!
//! The op request/response payload types are NOT generated here — they are
//! extern_path'd to `lit_actions_grpc::proto` (see build.rs), so this module
//! only adds the `GuestOps` service plus the `Job` envelope.
#![allow(clippy::unwrap_used, clippy::ignored_unit_patterns)]

tonic::include_proto!("com.litprotocol.actions.guest");

pub use guest_ops_client::GuestOpsClient;
pub use guest_ops_server::{GuestOps, GuestOpsServer};
