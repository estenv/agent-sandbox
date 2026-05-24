# Home folder access: current implementation

## Problem

The sandbox can read the entire host home directory. The SRT policy uses a
deny-list for specific credential paths (`~/.ssh`, `~/.aws`, etc.), but
everything else in `~` is implicitly readable. A prompt-injected agent could
browse personal files.

## Implemented solution

### Approach: block `~`, allow-read tool paths

The SRT policy now blocks `~` entirely and selectively allows specific
subdirectories that tools need:

```
denyRead:  ["~"]                                    → hides entire home with --tmpfs
allowRead: [<PATH-derived bins>, <well-known dirs>]  → re-binds specific tool dirs
allowWrite: [projects_root, ~/.agent-sandbox, ~/.cargo, /tmp, /dev/shm]
```

### What gets exposed

**From `$PATH` scanning:** Every PATH entry that lives under `~` is resolved
(canonicalized) and added to `allowRead`. This captures mise runtimes, cargo
tools, user scripts, etc. without hardcoding paths.

**Well-known state directories** (added unconditionally if they exist):
- `~/.local/share/mise` — mise-managed runtimes (node, python, etc.)
- `~/.local/bin` — user-local scripts

**Allow-write for cargo:** `~/.cargo` is in `allowWrite` so cargo can use the
host's registry cache (read and write). Since the sandbox has no internet
access, cargo credentials inside this directory cannot be exfiltrated.

### What stays hidden

Everything else under `~` that isn't in `allowRead` or `allowWrite`:
`~/.ssh/`, `~/.aws/`, `~/.config/gh/`, `~/.local/share/opencode/auth.json`,
`~/.local/share/keyrings/`, etc.

### PATH filtering inside sandbox

The host `$PATH` is filtered to remove entries under `~` that aren't in the
`allowRead` set. System paths (`/usr/bin`, `/usr/local/bin`, etc.) are
preserved. The sandbox home `bin/` directory is prepended.

### `prepare` installs into sandbox home

The `agent-sandbox prepare <agent>` command now runs:
```
npm install --prefix <sandbox_home>/npm-prefix -g <package>
```

And symlinks the binary into `<sandbox_home>/bin/<cmd>`. This puts agent
binaries in the writable sandbox home rather than relying on host tool paths.

### Cargo

`CARGO_HOME` is set to the host `~/.cargo` so cargo uses the host's registry
cache. The helper daemon also handles `cargo fetch` for projects with
`Cargo.toml` — running on the host (with network access) to populate the
cache when the sandbox can't reach crates.io.

### Edge cases

- **Non-existent `~/.cargo`**: The `~/.cargo` `allowWrite` entry is omitted
  if the directory doesn't exist on the host. `CARGO_HOME` is not set.
- **Symlinks in PATH**: Resolved via `canonicalize()` before adding to
  `allowRead`.
- **Mise shims vs. system binaries**: Both are covered — system binaries
  come from `/usr/bin/` (always accessible), mise shims from PATH scanning.

## Security model

| Threat | Mitigation |
|--------|-----------|
| Agent reads SSH keys | `~/.ssh/` blocked by `~` denyRead |
| Agent reads cloud creds | `~/.aws/`, `~/.azure/`, `~/.config/gcloud/` etc. blocked |
| Agent reads GitHub token | `~/.config/gh/` blocked |
| Agent reads opencode auth | `~/.local/share/opencode/` not in allowRead |
| Agent reads cargo token | `~/.cargo/credentials` is accessible (in allowWrite), but sandbox has no internet to use it |
| Agent writes to host home | Host home is read-only (not in `allowWrite`); only `~/.cargo` and `~/.agent-sandbox` are writable |
