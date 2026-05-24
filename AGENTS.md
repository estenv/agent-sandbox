# agent-sandbox

Wrapper around [srt](https://github.com/anthropic-experimental/sandbox-runtime) for sandboxed coding agents with credential protection and no outbound internet.

- `agent-sandbox` (src/): CLI — config, agent prep, srt policy generation, sandbox launch
- `agent-sandbox-helper-daemon` (helper-daemon/src/main.rs): Unix socket daemon for privileged host ops (git, packages, ADO)
- `agent-sandbox-helper` (helper-daemon/src/bin/helper.rs): Socket client used inside sandbox

Run `./verification.sh` after changes. Tests in `tests/` files (not `#[cfg(test)]`). Only test `pub` API from integration tests.
