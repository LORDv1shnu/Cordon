# Cordon — Command Reference

All commands, flags, and their combinations, current and planned.

---

## Legend

| Symbol | Meaning |
|--------|---------|
| `[IMPLEMENTED]` | Available right now |
| `[PLANNED — Phase N]` | Not yet implemented; belongs to the phase shown |
| `<value>` | Required user-supplied argument |
| `[value]` | Optional argument |

---

## Top-level help

```
cordon --help
cordon --version
cordon <subcommand> --help
```

Status: `[IMPLEMENTED]`

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
# Add a directory read-only
cordon add /home/user/assets --mode ro

# Add a directory read-write
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

All error cases where a user makes a typo or omits a required argument produce actionable output instead of raw clap errors:

### Unknown / misspelled subcommand

```
$ cordon statis
error: cordon statis — unknown subcommand

  Did you mean?  cordon status

  Usage:  cordon status
```

The closest matching command is found using Levenshtein edit distance (≤ 3 edits). If nothing is close enough, a hint to run `cordon --help` is shown instead.

### Missing required argument

```
$ cordon add
error: missing required argument: <PATH>

  Usage:  cordon add <path> [--mode <ro|rw>]

  Run cordon add --help for full details.
```

Full syntax is always shown for the command that was attempted.

---

## Config Files

| File | Location | Purpose |
|------|----------|---------|
| `core.toml` | embedded in binary | Module blueprint (immutable at runtime) |
| `system.toml` | `~/.config/cordon/system.toml` | Scanner output — verified system mount entries |
| `proxy.toml` | `./proxy.toml` or `~/.config/cordon/proxy.toml` | Domain allow-list for `--net=allow` |
| `cordon.toml` | `<project>/cordon.toml` (walks up) | Per-project user-defined mounts |
| `last-run.log` | `~/.config/cordon/logs/last-run.log` | Full `TRACE` log of the last run |

---

---

# Planned Future Commands

---

## `cordon status` — Show `system.toml` state without scanning

```
cordon status
```

Status: `[IMPLEMENTED — Phase 3]`

Displays the contents of `system.toml` without triggering a scan:

- Each module: name, verification status (`✅` / `⚠️`), source path, `when` category.
- Header: `last_scan` timestamp, `cordon_version`.

Useful for debugging "why isn't my module being mounted?" without running a full command.

---

## `cordon profile create` — Create a named run profile

```
cordon profile create <name> [--net <PROFILE>] [--gui] [--optional <MODULE>]...
```

Status: `[PLANNED — Phase 2.8]`

Creates a named profile stored in `~/.config/cordon/profiles.toml`:

```toml
[profile.GUI_APP]
network = "allow"
gui = true
optional = ["audio_pipewire", "dbus_session"]
```

---

## `cordon profile list` — List all profiles

```
cordon profile list
```

Status: `[PLANNED — Phase 2.8]`

Prints all profiles defined in `~/.config/cordon/profiles.toml` with their settings.

---

## `cordon profile delete` — Delete a profile

```
cordon profile delete <name>
```

Status: `[PLANNED — Phase 2.8]`

Removes the named profile from `profiles.toml`.

---

## `cordon run --profile` — Run with a named profile

```
cordon run --profile <name> -- <cmd> [args...]
```

Status: `[PLANNED — Phase 2.8]`

Loads the named profile's settings, then overlays any additional CLI flags on top.

**Resolution order (lowest → highest priority):**

```
built-in defaults → profile → cordon.toml → CLI flags
```

### Example

```bash
cordon run --profile GUI_APP -- discord
cordon run --profile GUI_APP --net=full -- discord  # CLI flag overrides profile
```

---

## `cordon run` with profile flags in `cordon.toml`

Status: `[PLANNED — Phase 2.7]`

Adds `network`, `gui`, and `optional` fields to `cordon.toml` so `cordon run -- <cmd>` with no CLI flags automatically applies the project's declared settings:

```toml
network = "allow"
gui = true
optional = ["audio_pipewire", "dbus_session"]

[[mount]]
src  = "/home/user/assets"
dest = "/home/user/assets"
mode = "ro"
```

CLI flags always override `cordon.toml` values.

---

## `cordon run` with per-project module overrides

```toml
# cordon.toml
require_optional = ["audio_pipewire"]
```

Status: `[PLANNED — Phase 4]`

Auto-activates the listed optional modules for every `cordon run` in this project — no need to pass `--optional` every time.

---

## `cordon run --quiet` / `--verbose`

```
cordon run --quiet -- <cmd>
cordon run --verbose -- <cmd>
```

Status: `[PLANNED — Phase 4]`

| Flag | Behaviour |
|------|-----------|
| `--quiet` | Suppress all Cordon output; only show sandboxed command output |
| `--verbose` | Print every bwrap argument on its own line; show each mount as it is applied |

---

## strace Integration

Status: `[PLANNED — Phase 3]`

Wraps bwrap with `strace` to capture blocked syscalls and paths, then parses and displays what the sandboxed app tried to access but could not. Writes a structured log of blocked paths after each run.

---

## Phase 5 — TUI

Status: `[PLANNED — Phase 5]`

Interactive terminal UI:

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
