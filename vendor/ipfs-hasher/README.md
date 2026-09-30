# ipfs-hasher 0.13.0 finalization fix

MIT-licensed implementation based on [ipfs-hasher 0.13.0](https://crates.io/crates/ipfs-hasher/0.13.0), authored by Wilfried Kopp.

The upstream wrapper discards blocks produced by `FileAdder::push` and unwraps
`finish().last()`. `finish` can be empty when a complete root was already emitted
by `push`, notably for a 262144-byte file. Retain the last CID from both stages.
The underlying UnixFS algorithm and parameters are unchanged.

The Keychain, API server, Actions, and Core Cargo roots patch this dependency to
this directory. `lit-agent-keychain/tests/protocol.rs` compares boundary/multichunk
results with independent JavaScript UnixFS vectors. Remove the patch only when an
upstream version passes those vectors.
