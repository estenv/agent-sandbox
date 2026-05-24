# agent-sandbox

Convenience wrapper around [Anthropic Sandbox Runtime](https://github.com/anthropic-experimental/sandbox-runtime) (`srt`) for running coding agents under a shared projects root with credential protection and no general outbound internet access.

## Modules

| Crate | Path | Purpose |
|-------|------|---------|
| `agent-sandbox` | `src/` | CLI wrapper: config, policy generation, agent prep, sandbox launch |
| `agent-sandbox-helper-daemon` | `helper-daemon/` | Host-side daemon for privileged operations, communicates over Unix sockets |
| `agent-sandbox-helper` | `helper-daemon/` | Unix socket client for agents inside the sandbox |

## Quick verification

Run `./verification.sh` after every change before considering it complete — it formats, lints with auto-fix, runs the basic test suite, and does a healthcheck.

## Test rules

- All tests in `tests/` files (per crate), not `#[cfg(test)]` inline modules.
- Only test `pub` API from integration tests.
- Never make a function `pub` just to test it. Use `#[cfg(test)]` modules for private methods.
