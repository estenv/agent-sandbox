# Home folder access: current analysis

## Problem

The sandbox can read the entire host home directory. The SRT policy uses a
deny-list for specific credential paths (`~/.ssh`, `~/.aws`, etc.), but
everything else in `~` is implicitly readable. A prompt-injected agent could
browse personal files.

## Goal

Restrict the sandbox to only see:
- `projects_root` (default `~/repos`) — read-write
- `sandbox_home` (default `~/.agent-sandbox`) — read-write
- System paths (`/usr`, `/bin`, `/lib`, `/etc`) — read-only (for tools)
- Tool installation paths under `~` — read-only (only what's needed)

Everything else in `~` should be invisible.

## Mechanism

SRT supports this via its `denyRead` + `allowRead`/`allowWrite` interaction:

```
denyRead:  ["/home/<user>"]   → hides entire home with --tmpfs
allowRead: ["<tool-paths>"]    → re-binds specific tool dirs on top
allowWrite: ["<writable>"]    → re-binds writable dirs on top
```

The `allowRead`/`allowWrite` entries survive the `denyRead` tmpfs overlay
because SRT re-binds them afterward (confirmed from SRT source).

## Challenge: tool paths are under `~`

On this system, virtually all development tools are under `~`:

| Tool | Path | Method |
|------|------|--------|
| `node`/`npm` | `~/.local/share/mise/installs/node/*/bin/` | mise |
| `python3` | `~/.local/share/mise/installs/python/*/bin/` | mise |
| `cargo`/`rustc` | `~/.cargo/bin/` | rustup |
| `opencode`/`pi` | `~/.local/share/mise/installs/node/*/bin/` | mise |
| `git` | `/usr/bin/git` | system |
| `az` | `/usr/bin/az` | system |

Blocking `~` entirely without adding tool paths to `allowRead` would break
the agent — it can't run node/npm/python/cargo or even itself.

## Proposed approach

At SRT policy render time, auto-discover tool paths from `$PATH`:

1. Add `"~"` to `denyRead` (blocks entire home)
2. Scan `$PATH` for entries under `~`, resolve symlinks, add to `allowRead`
3. Remove the now-redundant individual credential `denyRead` entries
4. Keep existing `allowWrite` entries (projects_root, sandbox_home, etc.)

This adapts to any tool installation method (mise, nvm, asdf, pyenv, rustup,
bun, pipx, etc.) because PATH already points at the tools the user needs.

## Edge cases

- **Tool runtime files outside PATH**: Some tools read state from non-PATH
  locations (e.g., mise reads `~/.local/share/mise/` for tool metadata).
  May need a small set of default `allowRead` entries for common runtimes.
- **Symlinks in PATH**: PATH entries may be symlinks to actual installations.
  Resolve them and add the canonical path to `allowRead`.
- **`srt` binary**: Lives at `~/.cache/.bun/bin/srt` but runs on the host,
  not inside the sandbox. Not affected.

## Status

Deferred. This will be implemented separately after the current round of
smaller fixes.
