# agent-sandbox

Convenience wrapper around [Anthropic Sandbox Runtime](https://github.com/anthropic-experimental/sandbox-runtime) (`srt`) for running coding agents under a shared projects root with credential protection and no general outbound internet access.

Agents run inside an `srt` sandbox that grants read/write access to all projects under a configured projects root (`~/repos` by default). A shared, persistent fake home (`~/.agent-sandbox`) allows agent config, auth, and skills to survive across sessions.

## Prerequisites

- Rust (1.75+)
- [`srt`](https://github.com/anthropic-experimental/sandbox-runtime) installed globally (`npm install -g @anthropic-ai/sandbox-runtime`)
- Agents installed on the host (e.g. `npm install -g opencode-ai`)

## Quick start

```bash
cargo install --path .
agent-sandbox init     # creates config, settings, and workspace dirs
cd ~/repos/my-project
agent-sandbox opencode # launches opencode in the sandbox
```

Review `~/.config/agent-sandbox/settings.json` before trusting the sandbox policy.

## CLI

```
agent-sandbox init
agent-sandbox prepare <agent>
agent-sandbox run [--projects-root <path>] [--workspace <path>] -- <command> [args...]
agent-sandbox pi [args...]
agent-sandbox opencode [args...]
agent-sandbox claude [args...]
agent-sandbox copilot [args...]
agent-sandbox doctor
```

### Subcommands

| Command | Description |
|---|---|
| `init` | Create default wrapper config, SRT settings, and runtime directories |
| `prepare` | Install/update a known agent on the host via npm |
| `run` | Run any command inside the sandbox |
| `pi` | Shortcut for `run -- pi` |
| `opencode` | Shortcut for `run -- opencode` |
| `claude` | Shortcut for `run -- claude` |
| `copilot` | Shortcut for `run -- copilot` |
| `doctor` | Sandboxed connectivity check against the helper daemon |

### CWD rules

- If the current working directory is inside the projects root, the sandbox inherits it.
- If the current working directory is **outside** the projects root, the sandbox CWD is set to the projects root itself.

This ensures agents only ever operate within the designated projects scope.

### Configuration

Wrapper config at `~/.config/agent-sandbox/config.toml`:

```toml
projects_root = "~/repos"
sandbox_home = "~/.agent-sandbox"
```

| Variable | Overrides | Default |
|---|---|---|
| `AGENT_SANDBOX_PROJECTS_ROOT` | `projects_root` | `~/repos` |
| `AGENT_SANDBOX_HOME` | `sandbox_home` | `~/.agent-sandbox` |
| `AGENT_SANDBOX_SETTINGS` | SRT settings path | `~/.config/agent-sandbox/settings.json` |
| `AGENT_SANDBOX_NPM` | npm binary for agent prep | `npm` |

### Sandbox-visible runtime state

The sandbox home (`~/.agent-sandbox` by default) contains synthetic HOME, XDG config/cache/data, temp, and npm directories. LLM provider auth lives here and persists across sessions. Real host credentials (SSH, GitHub, cloud, etc.) remain denied by the SRT policy.

```
~/.agent-sandbox/
├── home/
├── config/
├── cache/
├── share/
├── tmp/
├── npm-cache/
├── npm-prefix/
├── bin/
└── logs/
```

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
