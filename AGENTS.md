# agent-sandbox

Convenience wrapper around [Anthropic Sandbox Runtime](https://github.com/anthropic-experimental/sandbox-runtime) (`srt`) for running coding agents under a shared projects root with credential protection and no general outbound internet access.

## Modules

| Crate | Path | Purpose |
|-------|------|---------|
| `agent-sandbox` | `src/` | CLI wrapper: config, policy generation, agent prep, sandbox launch |
| `agent-sandbox-helper-daemon` | `helper-daemon/` | Host-side daemon for privileged operations, communicates over Unix sockets |
| `agent-sandbox-helper` | `helper-daemon/` | Unix socket client for agents inside the sandbox |
| `sandbox-tests` | `sandbox-tests/` | End-to-end tests that run inside the real sealed sandbox via `srt` |

The daemon exposes actions as simple text commands over a Unix socket.
Actions currently available: `healthz`, `test`, `git-pull <absolute-path>`.

- `git-pull` runs `git pull` on the **host** (bypassing sandbox network restrictions). The path must be absolute and within the configured projects root.
- Agents inside the sandbox invoke actions via `agent-sandbox-helper <action> [args...]`.

## Quick verification after changes

```bash
# Run all tests
cargo test --workspace

# Run all tests including sealed sandbox tests (requires srt on PATH):
cargo test --workspace

# Auto-format code
cargo fmt

# Auto-fix clippy lint issues
cargo clippy --fix --allow-dirty

# Healthcheck (sandbox-daemon connectivity):
cargo build --workspace
cargo run -- healthcheck
```

The `sandbox-tests` crate runs end-to-end sealed tests via `srt`. These tests are
not filtered out by any `#[ignore]` — they require `srt` to be on `PATH` at
compile time and will fail with a clear error message if it is missing. If you
do not need to run them, use `cargo test --workspace --exclude sandbox-tests` or
`cargo test --workspace --exclude sandbox-tests --exclude helper-daemon`.

All scenarios in `sandbox-tests` run inside a single test function (not parallel)
to avoid resource contention between concurrent sandbox sessions.

## Test rules

- **All tests must live in `tests/` files** (per crate), never as `#[cfg(test)]` inline modules in source files.
- Integration tests go in `tests/` under the crate root (`agent-sandbox/tests/`, `helper-daemon/tests/`).
- Only test `pub` API from integration tests.
- Never make a function `pub` just to test it. If a private method has a high-value test, use an inline `#[cfg(test)]` module instead.
