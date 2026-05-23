# Agent Sandbox

## Purpose

Agent Sandbox is a convenience wrapper around Anthropic Sandbox Runtime (`srt`) for running coding agents with a practical daily workflow. It does not replace `srt`; it makes `srt` easier to use consistently.

The target workflow is:

```bash
agent-sandbox init
agent-sandbox clone git@github.com:org/repo.git
cd repo
agent-sandbox pi
agent-sandbox opencode
agent-sandbox copilot
agent-sandbox run -- bash
```

The wrapper handles:

- default config discovery,
- default sandbox-visible runtime state,
- known-agent preparation,
- command shortcuts,
- and a future integration point for a host-side helper daemon.

The security boundary remains:

```bash
srt --settings ~/.config/agent-sandbox/settings.json -- <command>
```

## Defaults

- SRT settings: `~/.config/agent-sandbox/settings.json`.
- Sandbox-visible runtime state: `~/.agent-sandbox`.
- Helper daemon test endpoint: `http://localhost:47688/healthz`.

The runtime state directory contains:

```text
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

The sandboxed command runs with:

- `HOME=~/.agent-sandbox/home`
- `XDG_CONFIG_HOME=~/.agent-sandbox/config`
- `XDG_CACHE_HOME=~/.agent-sandbox/cache`
- `XDG_DATA_HOME=~/.agent-sandbox/share`
- `TMPDIR=~/.agent-sandbox/tmp`
- npm cache/prefix under `~/.agent-sandbox`

LLM provider auth is allowed to live in the sandbox-visible agent home. Real host credentials such as SSH keys, GitHub CLI auth, cloud credentials, package-manager tokens, and real home config remain denied by the default SRT policy.

## CLI shape

```bash
agent-sandbox init
agent-sandbox clone <repo-url> [directory]
agent-sandbox prepare <agent>
agent-sandbox run -- <command> [args...]
agent-sandbox pi [args...]
agent-sandbox opencode [args...]
agent-sandbox claude [args...]
agent-sandbox copilot [args...]
agent-sandbox doctor
```

`doctor` currently runs a sandboxed `curl` to the helper daemon health endpoint.

## Settings ownership

`agent-sandbox init` can create a starter policy, but it cannot know the right policy for every machine or workflow. The user must review and edit:

```text
~/.config/agent-sandbox/settings.json
```

before trusting it.

## Policy model

### Filesystem

SRT reads are deny-then-allow. By default, reads are allowed everywhere; `filesystem.denyRead` blocks specific paths, and `filesystem.allowRead` can re-allow paths inside denied regions.

The default policy intentionally does not deny all of `/home`, because that would require re-allowing the workspace. Since `allowRead` takes precedence over `denyRead`, broad home denial plus workspace re-allow can make it difficult to deny individual secret files inside the workspace.

Instead, the policy explicitly denies common sensitive files and directories:

- SSH and GPG material.
- GitHub, Azure, Docker, Kubernetes, AWS, and GCP credentials.
- Package-manager credentials for npm, PyPI, Cargo, NuGet, Maven, and Gradle.
- OpenCode's documented auth file at `~/.local/share/opencode/auth.json`.
- Common project-local secret files such as `.env`, `.env.local`, `.envrc`, `.npmrc`, `.pypirc`, and NuGet config files.

Write access uses SRT's allow-only model. The policy allows writes to `.` and `/tmp`, then explicitly denies writes to common secret/config files inside the workspace.

The default policy further blocks `allowGitConfig: false` to prevent git config mutation.

### Network

SRT network access is allow-only by default. The default policy allowlists only `api.anthropic.com` for the initial Anthropic provider path, plus `localhost` and `127.0.0.1` for the helper daemon.

The explicit deny list repeats high-risk destinations even though they are already blocked by omission. This makes the policy easier to audit and protects against accidental future broad allow rules because `deniedDomains` takes precedence over `allowedDomains`.

The policy does not allow package registries or source-control remotes. Dependency installation, `git fetch`, `git push`, PR creation, and Azure DevOps operations belong in the future helper-daemon track, not inside the sandbox itself.

### Unix sockets

The default policy does not allow Unix sockets. `allowUnixSockets` is an empty array and `allowAllUnixSockets` is `false`. On Linux, SRT blocks Unix socket creation with seccomp where supported. The future helper daemon should not assume Unix sockets are the transport.

## Preparation model

`agent-sandbox run -- <command>` runs an automatic preparation step when the command is not found on the host:

- If `opencode` is missing, install `opencode-ai` globally with npm.
- If `pi` or `pi-agent` is missing, install `@mariozechner/pi-coding-agent` globally with npm.
- If `claude` is missing, install `@anthropic-ai/claude-code` globally with npm.

Preparation happens outside `srt`. The sandbox never gets npm registry access.

Use `agent-sandbox run --no-prepare -- <command>` to skip auto-installation.

## Auth stance

LLM provider auth is allowed to live under `~/.agent-sandbox`, either as environment-provided credentials or as agent-specific auth files written to the fake home.

This is an accepted risk because provider-token exfiltration has limited blast radius compared with host SSH keys, GitHub tokens, cloud credentials, package-manager credentials, or repository mutation credentials.

The default settings still deny access to real host credential paths.

## OpenCode-specific notes

OpenCode stores credentials at `~/.local/share/opencode/auth.json` by default — this path is denied by the default SRT policy. Use provider API keys from environment variables instead of agent-specific login files in the real home directory.

Set these environment variables before launch to avoid startup dependencies on blocked network domains:

```bash
OPENCODE_DISABLE_AUTOUPDATE=true
OPENCODE_DISABLE_LSP_DOWNLOAD=true
OPENCODE_DISABLE_MODELS_FETCH=true
```

The wrapper sets these automatically when launching opencode.

## Workspace layout

This repository is a Rust workspace with two crates:

```text
agent-sandbox/
├── Cargo.toml
├── src/
│   └── main.rs
├── helper-daemon/
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
└── PROJECT.md
```

## Helper daemon skeleton

The helper daemon is intentionally separate from the `srt` wrapper. It is a host-side service for future privileged operations that should not run inside the sandbox.

Current skeleton:

```bash
cargo run -p agent-sandbox-helper-daemon
```

Default bind:

```text
127.0.0.1:47688
```

Endpoints:

```text
GET /healthz
GET /v1/test
```

Both endpoints only return static JSON. They do not execute commands, mutate repositories, install packages, or call external APIs.

## Helper daemon future scope

Later phases can add:

- controlled remote Git operations,
- GitHub PR/issue operations,
- Azure DevOps work item operations,
- controlled package installation,
- audit logging,
- and an agent skill/tool description that teaches agents how to request those operations.

The daemon should remain host-mediated and policy-driven. The sandbox should not receive GitHub, Azure DevOps, SSH, or package-manager credentials directly.

## Connectivity test

In one terminal:

```bash
cargo run -p agent-sandbox-helper-daemon
```

In a project directory:

```bash
agent-sandbox doctor
```

Expected result:

```json
{"ok":true,"service":"agent-sandbox-helper-daemon"}
```

If this fails, inspect the `network.allowedDomains` section in the SRT settings and confirm it includes `localhost` and `127.0.0.1`.

## Known limitations

- Linux path matching is literal, not glob-based. The policy uses explicit paths only.
- SRT read restrictions are not a complete "only this repository is readable" policy unless you deny broad parent directories and carefully re-allow required paths.
- Broadly denying `/home` is possible, but it changes the trade-off: workspace re-allow may override attempts to block individual project-local secret files.
- If opencode requires additional provider domains or state paths on the target machine, add only the narrow domains/paths actually observed in testing.
- If the target provider is not Anthropic, replace `api.anthropic.com` with the exact provider endpoint and keep the rest of the network policy closed.
