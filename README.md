# agent-sandbox

Convenience wrapper around [Anthropic Sandbox Runtime](https://github.com/anthropic-experimental/sandbox-runtime) (`srt`) for running coding agents with workspace access, credential protection, and no general outbound internet access.

## Prerequisites

- Rust (1.75+)
- [`srt`](https://github.com/anthropic-experimental/sandbox-runtime) installed globally (`npm install -g @anthropic-ai/sandbox-runtime`)
- Agents installed on the host (e.g. `npm install -g opencode-ai`)

## Quick start

```bash
cargo install --path .
agent-sandbox init
agent-sandbox run -- opencode
```

Review `~/.config/agent-sandbox/settings.json` before trusting the sandbox policy.

## CLI

```
agent-sandbox init
agent-sandbox clone <repo-url> [directory]
agent-sandbox prepare <agent>
agent-sandbox run [--settings <path>] [--workspace <path>] -- <command> [args...]
agent-sandbox pi [args...]
agent-sandbox opencode [args...]
agent-sandbox claude [args...]
agent-sandbox copilot [args...]
agent-sandbox doctor
```

### Subcommands

| Command | Description |
|---|---|
| `init` | Create default SRT settings and sandbox runtime directories |
| `clone` | Clone a Git repository on the host (outside the sandbox) |
| `prepare` | Install/update a known agent on the host via npm |
| `run` | Run any command inside the sandbox |
| `pi` | Shortcut for `run -- pi` |
| `opencode` | Shortcut for `run -- opencode` |
| `claude` | Shortcut for `run -- claude` |
| `copilot` | Shortcut for `run -- copilot` |
| `doctor` | Sandboxed connectivity check against the helper daemon |

### Environment

| Variable | Default | Purpose |
|---|---|---|
| `AGENT_SANDBOX_SETTINGS` | `~/.config/agent-sandbox/settings.json` | Path to SRT settings file |
| `AGENT_SANDBOX_HOME` | `~/.agent-sandbox` | Sandbox-visible runtime state root |
| `AGENT_SANDBOX_NPM` | `npm` | npm command for host-side agent preparation |

The sandbox-visible runtime state (`~/.agent-sandbox` by default) contains synthetic HOME,
XDG config/cache/data, temp, and npm directories. LLM provider auth lives here.
Real host credentials (SSH, GitHub, cloud, etc.) remain denied by the SRT policy.

## Project layout

```
agent-sandbox/
├── Cargo.toml
├── src/main.rs
├── helper-daemon/
│   ├── Cargo.toml
│   └── src/main.rs
└── PROJECT.md
```
