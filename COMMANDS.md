# Cordon — Command Reference

> **Reading order:** [README.md](README.md) → **COMMANDS.md** → [SCANNER_LOGIC.md](SCANNER_LOGIC.md) → [MODULE_INFO.md](MODULE_INFO.md) → [PROGRESS.md](PROGRESS.md)
>
> New to Cordon? Start with [README.md](README.md) first. This document is the exhaustive flag reference.

---

## Legend

| Symbol | Meaning |
|--------|---------|
| `[IMPLEMENTED]` | Available right now |
| `[PLANNED — Phase N]` | Not yet implemented; belongs to the phase shown |
| `<value>` | Required user-supplied argument |
| `[value]` | Optional argument |

---

## `cordon run` — Run a command inside the sandbox

```
cordon run [FLAGS] -- <cmd> [args...]
```

Status: `[IMPLEMENTED — Phase 1]`

> **`--`** is required to separate Cordon flags from the command being sandboxed.

### Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--net <PROFILE>` | enum | `disable` | Network access profile (see Network Profiles below) |
| `--domain <DOMAIN>` | string (repeatable) | — | Allow a specific domain through the proxy. Requires `--net=allow`. Can be repeated or comma-separated |
| `--gui` | bool | `false` | Enable GUI app support — mounts X11 socket, Wayland runtime, fontconfig |
| `--optional <MODULE>` | string (repeatable) | — | Activate an optional module by name (e.g. `audio_pipewire`, `dbus_session`) |
| `--profile <NAME>` | string | — | Load a named profile from `profiles.toml` (e.g. `python`, `gui-app`) |
| `--dry-run` | bool | `false` | Print the full bwrap command and exit without executing it |
| `--debug` | bool | `false` | Enable verbose tracing logs on stderr; always writes to `~/.config/cordon/logs/last-run.log` |

### Network Profiles (`--net`)

| Value | Behaviour |
|-------|-----------|
| `disable` | No network access at all (default). Full network namespace isolation via `--unshare-net`. |
| `allow` | Filtered network via built-in HTTP/HTTPS proxy. Only domains in `proxy.toml` (or `--domain`) reachable. |
| `full` | Unrestricted internet access — shares the host network namespace. |

### Optional Modules (`--optional`)

| Module Name | What It Enables |
|-------------|-----------------|
| `home_config` | Read-write access to `$HOME` |
| `locale_files` | System locale / i18n data (`/usr/share/locale`) |
| `timezone` | Local timezone symlink (`/etc/localtime`) |
| `ld_so_cache` | Dynamic-linker cache (`/etc/ld.so.cache`) — faster startup |
| `audio_pipewire` | PipeWire audio socket |
| `audio_pulse` | PulseAudio legacy audio socket |
| `dbus_session` | D-Bus session bus (clipboard, notifications, IPC) |
| `dconf_runtime` | GNOME dconf preferences runtime dir |
| `gpu_dri` | GPU/DRI device files for hardware-accelerated rendering |

### Examples

```bash
# Run with no network (default)
cordon run -- python3 script.py

# Run with full network
cordon run --net=full -- npm install

# Run with filtered proxy, only allowing github.com
cordon run --net=allow --domain github.com -- curl https://github.com

# Run with filtered proxy, domains from proxy.toml + extra
cordon run --net=allow -- cargo build

# Run a GUI app with sound
cordon run --gui --optional audio_pipewire -- firefox

# Run a GUI app with D-Bus and GPU
cordon run --gui --optional dbus_session --optional gpu_dri -- discord

# Dry-run: inspect the bwrap command without executing
cordon run --dry-run -- python3 script.py

# Debug mode: verbose tracing on stderr
cordon run --debug -- node server.js
```

---

## `cordon scan` — Scan the system and generate `system.toml`

```
cordon scan
```

Status: `[IMPLEMENTED — Phase 2]`

Runs the interactive full scanner. Probes all module paths on disk, detects symlinks vs real directories, asks the user about network / GUI / optional modules, and writes `~/.config/cordon/system.toml`.

Triggered automatically on first `cordon run` when `system.toml` is missing.

### Scanner Phases

| Phase | Modules scanned | User interaction |
|-------|----------------|-----------------|
| 1 | `always` (mandatory) — `/usr`, `/bin`, `/lib`, `/lib64`, `/sbin` | None — auto-detected |
| 2 | `network` — `resolv_conf`, `dns_resolution`, `ssl_certificates` | Prompt: "Include network support?" |
| 3 | `gui` — `x11_socket`, `fonts`, `wayland_runtime`, `dconf_runtime`, `dbus_session`, `gpu_dri` | Prompt: "Include GUI support?" |
| 4 | `optional` — home, locale, timezone, ld.so.cache, audio, dbus, dconf, GPU | Per-module opt-in prompts |

> For a deep-dive into scanner internals, see [SCANNER_LOGIC.md](SCANNER_LOGIC.md).

---

## `cordon check` — Pre-flight health check

```
cordon check
```

Status: `[IMPLEMENTED — Phase 3]`

Runs 7 checks against the sandbox stack and prints a colour-coded table. Exits `0` if all checks pass, `1` if any check fails.

| # | Check | FAIL condition |
|---|-------|---------------|
| 1 | `bwrap installed` | `bwrap` not in PATH |
| 2 | `user namespaces` | bwrap cannot create a userns |
| 3 | `AppArmor userns` | `apparmor_restrict_unprivileged_userns = 1` |
| 4 | `system.toml` | File missing or malformed |
| 5 | `core modules` | Any required `always` module unverified |
| 6 | `network modules` | Any required `network` module unverified |
| 7 | `GUI modules` | Any required `gui` module unverified |

---

## `cordon list` — List active mounts

```
cordon list
```

Status: `[IMPLEMENTED — Phase 3]`

Displays all mounts that would be applied in the next sandbox run, without executing anything.

- **System mounts** — from `~/.config/cordon/system.toml`, grouped by `when` (always / network / gui / optional), with `✓` / `✗` verification indicators.
- **Project mounts** — from `./cordon.toml` (walked up from CWD), with path-existence indicators.

---

## `cordon status` — Show `system.toml` state

```
cordon status
```

Status: `[IMPLEMENTED — Phase 3]`

Displays the contents of `system.toml` without triggering a scan:

- Each module: name, verification status (`✅` / `⚠️`), source path, `when` category.
- Header: `last_scan` timestamp, `cordon_version`.

Useful for debugging "why isn't my module being mounted?" without running a full command.

---

## `cordon add` — Add a project mount

```
cordon add <path> [--mode <MODE>]
```

Status: `[IMPLEMENTED — Phase 2.5]`

Appends a new mount entry to the nearest `cordon.toml` (creates it if absent). The path is stored as its canonical absolute form — same for `src` and `dest`, so the path appears at the same location inside the sandbox.

| Flag | Default | Values |
|------|---------|--------|
| `--mode` | `ro` | `ro` (read-only), `rw` (read-write) |

### Examples

```bash
cordon add /home/user/assets --mode ro
cordon add /tmp/scratch --mode rw
```

---

## `cordon remove` — Remove a project mount

```
cordon remove <path>
```

Status: `[IMPLEMENTED — Phase 2.5]`

Removes a mount entry from `cordon.toml` by canonical path. If all entries are removed, the file is deleted.

---

## `cordon edit` — Open `cordon.toml` in the system editor

```
cordon edit
```

Status: `[IMPLEMENTED — Phase 2.5]`

Opens the nearest `cordon.toml` in `$EDITOR` (falls back to `vi`). Creates a blank `cordon.toml` in CWD if none exists.

---

## `cordon set` — Set project profile defaults

```
cordon set [--net <PROFILE>] [--gui] [--optional <MODULE>]
```

Status: `[IMPLEMENTED — Phase 2.7]`

Persists runtime flags into the nearest `cordon.toml` as project profile defaults. On subsequent `cordon run` calls with no flags, these values are applied automatically. **CLI flags always override cordon.toml values.**

### Example

```bash
# Set network + GUI + audio as project defaults
cordon set --net=allow --gui --optional audio_pipewire

# cordon.toml now contains:
# network = "allow"
# gui = true
# optional = ["audio_pipewire"]

# This run uses the profile — no flags required
cordon run -- discord
```

---

## `cordon unset` — Remove project profile defaults

```
cordon unset [--net] [--gui] [--optional <MODULE>]
```

Status: `[IMPLEMENTED — Phase 2.7]`

Removes specific profile defaults from `cordon.toml`. Mounts and other fields are not touched.

### Example

```bash
cordon unset --gui --optional audio_pipewire
```

---

## Config Files

| File | Location | Purpose |
|------|----------|---------|
| `core.toml` | embedded in binary | Module blueprint (immutable at runtime) |
| `system.toml` | `~/.config/cordon/system.toml` | Scanner output — verified system mount entries |
| `proxy.toml` | `./proxy.toml` or `~/.config/cordon/proxy.toml` | Domain allow-list for `--net=allow` |
| `cordon.toml` | `<project>/cordon.toml` (walks up) | Per-project mounts and profile defaults |
| `last-run.log` | `~/.config/cordon/logs/last-run.log` | Full `TRACE` log of the last run |

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success — sandboxed command exited cleanly |
| `1` | Cordon internal error (scan failure, config error, etc.) |
| `2` | Cordon usage error (bad CLI args) |
| `125` | Sandbox setup failed (bwrap not found, namespace error) |
| `126` | Sandboxed command found but not executable |
| `127` | Sandboxed command not found inside sandbox |
| `N` | Any other code — forwarded directly from the sandboxed process |

---

## Smart Error Suggestions

Status: `[IMPLEMENTED — Phase 3]`

All error cases where a user makes a typo or omits a required argument produce actionable output instead of raw clap errors.

```
$ cordon statis
error: cordon statis — unknown subcommand

  Did you mean?  cordon status

  Usage:  cordon status
```

The closest matching command is found using Levenshtein edit distance (≤ 3 edits).

---

## `cordon profile create` / `list` / `delete` / `show`

```
cordon profile create <name> [--net <PROFILE>] [--gui] [--optional <MODULE>]...
cordon profile list
cordon profile delete <name>
cordon profile show <name>
```

Status: `[IMPLEMENTED — Phase 2.8]`

Named profiles stored in `~/.config/cordon/profiles.toml`. Use with `cordon run --profile <name>`.

Resolution order (lowest → highest priority):
```
built-in defaults → named profile → cordon.toml → CLI flags
```

**Built-in profiles:**
| Name | net | gui | optional |
|------|-----|-----|----------|
| `python` | allow | — | `ld_so_cache`, `locale_files` |
| `node` | allow | — | `ld_so_cache`, `home_config` |
| `rust` | allow | — | `ld_so_cache` |
| `gui-app` | — | true | `audio_pipewire`, `dbus_session`, `gpu_dri` |

---

# Planned Future Commands

---

## `cordon run --quiet` / `--verbose`

Status: `[PLANNED — Phase 4]`

| Flag | Behaviour |
|------|-----------|
| `--quiet` | Suppress all Cordon output; only show sandboxed command output |
| `--verbose` | Print every bwrap argument on its own line; show each mount as it is applied |

---

## strace Integration

Status: `[PLANNED — Phase 3]`

Wraps bwrap with `strace` to capture blocked syscalls and paths, then parses and displays what the sandboxed app tried to access but could not.

---

## Phase 5 — TUI

Status: `[PLANNED — Phase 5]`

- Directory picker for mounts
- Toggle network / gui / dry-run visually
- View mounts before running
- Edit `cordon.toml` entries via TUI

---

## Phase 6 — Profiles & Distribution

Status: `[PLANNED — Phase 6]`

| Feature | Notes |
|---------|-------|
| Built-in profiles | `nodejs`, `python`, `rust` — pre-configured optional module sets |
| GitHub Actions CI | Smoke tests on `x86_64-unknown-linux-gnu` |
| Prebuilt binaries | Binary releases via GitHub Releases |

---

## Next

→ [SCANNER_LOGIC.md](SCANNER_LOGIC.md) — how the scanner and integrity check work internally
