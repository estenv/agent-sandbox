#!/usr/bin/env bash
set -euo pipefail

cargo fmt
cargo clippy --fix --allow-dirty
cargo test --workspace
cargo build --workspace && cargo run -- healthcheck
