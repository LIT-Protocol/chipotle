[group: 'build']
build: actions api-server

[group: 'build']
actions:
    cargo build --manifest-path=lit-actions/Cargo.toml --bin lit_actions --all-features

[group: 'build']
api-server:
    cargo build --manifest-path=lit-api-server/Cargo.toml --bin lit-api-server --all-features

clean:
    cargo clean --manifest-path=lit-actions/Cargo.toml
    cargo clean --manifest-path=lit-api-server/Cargo.toml
