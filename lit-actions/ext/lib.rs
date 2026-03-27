pub mod bindings;
mod macros;

// Export extension
pub use bindings::lit_actions;

/// Pre-fetched lit action private key passed from the API server to avoid
/// a gRPC round-trip during execution. Stored in OpState.
pub struct PrefetchedLitActionKey(pub String);

/// Local accumulator for response and logs, avoiding gRPC round-trips for
/// setResponse and print ops. Retrieved after execution completes.
#[derive(Default)]
pub struct LocalExecutionState {
    pub response: String,
    pub logs: String,
}
