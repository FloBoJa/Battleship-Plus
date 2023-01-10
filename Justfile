fmt:
    cargo fmt --all -- --check

check:
    cargo check --package battleship_plus_server
    cargo check --package battleship_plus_client
    cargo check --package battleship_plus_common
    cargo check --package battleship_plus_macros
    cargo check --package bevy_quinnet

clippy:
    cargo clippy --package battleship_plus_server -- -D warnings
    cargo clippy --package battleship_plus_client -- -D warnings
    cargo clippy --package battleship_plus_common -- -D warnings
    cargo clippy --package battleship_plus_macros -- -D warnings
    cargo clippy --package bevy_quinnet -- -D warnings

test:
    cargo test --package battleship_plus_server
    cargo test --package battleship_plus_client
    cargo test --package battleship_plus_common
    cargo test --package battleship_plus_macros
    cargo test --package bevy_quinnet --all-features
