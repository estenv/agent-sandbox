# agent-sandbox

Convenience wrapper around [Anthropic Sandbox Runtime](https://github.com/anthropic-experimental/sandbox-runtime) (`srt`) for running coding agents under a shared projects root with credential protection and no general outbound internet access.

## Modules

| Crate | Path | Purpose |
|-------|------|---------|
| `agent-sandbox` | `src/` | CLI wrapper: config, policy generation, agent prep, sandbox launch |
| `agent-sandbox-helper-daemon` | `helper-daemon/` | Host-side HTTP service for future privileged operations |

## Quick verification after changes

```bash
# Run all tests
cargo test

# Auto-format code
cargo fmt

# Auto-fix clippy lint issues
cargo clippy --fix --allow-dirty

# Healthcheck (sandbox-daemon connectivity):
# Terminal 1:
cargo run -p agent-sandbox-helper-daemon
# Terminal 2:
cargo run -- doctor
```
