# agent-sandbox

Convenience wrapper around [Anthropic Sandbox Runtime](https://github.com/anthropic-experimental/sandbox-runtime) (`srt`) for running coding agents under a shared projects root with credential protection and no general outbound internet access.

## Modules

| Crate | Path | Purpose |
|-------|------|---------|
| `agent-sandbox` | `src/` | CLI wrapper: config, policy generation, agent prep, sandbox launch |
| `agent-sandbox-helper-daemon` | `helper-daemon/` | Host-side daemon for privileged operations, communicates over Unix sockets |
| `agent-sandbox-helper` | `helper-daemon/` | Unix socket client for agents inside the sandbox |

## Quick verification after changes

```bash
# Run all tests
cargo test

# Auto-format code
cargo fmt

# Auto-fix clippy lint issues
cargo clippy --fix --allow-dirty

# Healthcheck (sandbox-daemon connectivity):
cargo build --workspace
cargo run -- healthcheck
```

## Test rules

- **All tests must live in `tests/` files** (per crate), never as `#[cfg(test)]` inline modules in source files.
- Integration tests go in `tests/` under the crate root (`agent-sandbox/tests/`, `helper-daemon/tests/`).
- Only test `pub` API from integration tests.
- Never make a function `pub` just to test it. If a private method has a high-value test, use an inline `#[cfg(test)]` module instead.
