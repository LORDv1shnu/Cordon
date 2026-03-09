# Cordon — Progress Tracking

All completed work and planned work lives here.

---

## Completed

### Phase 1 — Core Sandbox ✅

| Feature | Notes |
|---------|-------|
| `cordon run -- <cmd>` CLI | clap subcommand, `--` separator |
| Spawn bubblewrap sandboxed process | namespace isolation, no root |
| System dirs mounted read-only | `/usr`, `/bin`, `/lib`, `/lib64`, `/sbin` |
| Project directory writable | `--bind <cwd> <cwd>` |
| `src/` protected as read-only overlay | auto-detected, `--ro-bind` |
| Network disabled by default | `--unshare-net`; `--network` flag to opt in |
| Portable symlink detection | stores raw target for `--symlink` bwrap arg |
| Dry-run mode (`--dry-run`) | prints full bwrap command, does not execute |
| GUI support (`--gui`) | X11/Wayland/fontconfig mounts + env vars |
| Narrowed `/etc` exposure | only specific files bound in network mode |
| bwrap not installed detection | clear error + install instructions, exit 125 |
| Clean exit code forwarding | child exit code forwarded exactly via encoded anyhow error |
| Refactored into modules | `cli.rs`, `config.rs`, `scanner/`, `sandbox/` |

---

### Phase 2 — Scanner & Policy System ✅

| Feature | Notes |
|---------|-------|
| `CoreModule`, `CoreConfig`, `SystemConfig`, `MountEntry` structs | in `config.rs` |
| `core.toml` module list (always / network / gui / optional) | compiled into binary |
| `include_str!()` embedding of `core.toml` | tamper-proof at runtime |
| Two-mode scanner architecture | `full_scan()` interactive + `integrity_check()` pre-flight |
| Phase 1–4 full scan (mandatory → network → GUI → optional) | with user prompts |
| Symlink detection in scanner | `bind_type = "symlink"` stored in `system.toml` |
| `verified = false` for missing/incomplete modules | never passed to bwrap |
| Corrected path prompt for missing required modules | handles NixOS / non-FHS |
| `integrity_check()` — 7-step pre-flight | see SCANNER_LOGIC.md for step breakdown |
| Foreign entry detection in `system.toml` | hard block — security gate |
| Version mismatch detection | triggers fresh `full_scan` |
| Malformed `system.toml` handling | triggers fresh `full_scan` |
| Broken path detection (post-upgrade) | triggers fresh `full_scan` |
| Required `always` module gate | hard block if unverified |
| `--network` gate | hard block if required network module unverified |
| `--gui` gate | hard block if required GUI module unverified |
| `fd-lock` write lock on `system.toml` | prevents concurrent scan corruption |
| `cordon.toml` discovery (walks up dir tree) | stops at `/` |
| `cordon scan` subcommand | manually triggers `full_scan()` |
| Auto-trigger scan on first `cordon run` | when `system.toml` is missing |
| All bwrap paths read from `system.toml` | zero hardcoded paths in sandbox module |
| Exit code strategy documented | codes 0 / 1 / 2 / 125 / 126 / 127 / N |

---

### Phase 2.5 — Runtime Environment Support ✅

| Feature | Notes |
|---------|-------|
| `--optional <module>` flag | activates optional modules at runtime; warns if unverified |
| `cordon add <path> --mode <rw\|ro>` | appends to `cordon.toml`; src=dest=canonical absolute path |
| Safe env var passthrough | `HOME`, `USER`, `LOGNAME`, `LANG`, `LC_ALL`, `PATH`, `XDG_*` |
| GUI env vars | `DISPLAY`, `WAYLAND_DISPLAY` forwarded only with `--gui` |
| `cordon.toml` confirmation prompt | `Enter=yes / N=no / D=show paths`; never applied silently |
| Dry-run includes `cordon.toml` mounts | so printed command is complete |
| Renamed scanner entry points | `full_scan()` / `integrity_check()` |

---

### Phase 2.6 — Scanner Completion (Partial) ✅

| Feature | Notes |
|---------|-------|
| D-Bus socket resolution at scan time | `resolve_dbus_socket()` in `env_resolver.rs` — reads `$DBUS_SESSION_BUS_ADDRESS`, strips `unix:path=` prefix, falls back to `$XDG_RUNTIME_DIR/bus` |
| `dbus_session` special-case in scanner | `scan_module_interactive()` calls `resolve_dbus_socket()` before the generic resolver |

---

### Code Quality Pass ✅

| Area | Change |
|------|--------|
| `config.rs` `add_user_mount` | Fixed broken dest (was `/project/<path>`, now canonical absolute path) |
| `config.rs` `find_user_config` | Added `with_context` error annotations; fixed double `/` stop condition |
| `scanner/env_resolver.rs` | Removed stale `#[allow(clippy::collapsible_if)]`; simplified guard order |
| `scanner/integrity.rs` | CORE_TOML parsed once at top; added Step 5 required-always gate; improved error messages |
| `sandbox/mounts.rs` | Removed dead double `verified` check; correct optional warning; `cordon.toml` errors surfaced as warnings |
| `sandbox/executor.rs` | bwrap check uses `bwrap --version` instead of `which bwrap` |

---

## Pending

### Phase 2.6 — Scanner Completion `[IN PROGRESS]`

**GPU/DRI Device Node Support**
- Detect `/dev/dri/` device nodes (`card0`, `renderD128`) at scan time.
- Add `"dev-bind"` as a valid `bind_type` in `MountEntry` and the bwrap mount loop in `mounts.rs`.
- `gpu_dri` module already exists in `core.toml` — scanner and sandbox just need `dev-bind` wired in.
- Without this: GPU acceleration unavailable, apps fall back to slow software rendering (llvmpipe).

**Audio Socket Resolution at Scan Time**
- Resolve `$PIPEWIRE_RUNTIME_DIR` / `$PULSE_RUNTIME_PATH` at scan time, same pattern as `resolve_dbus_socket()`.
- Store the resolved socket path in `system.toml`.
- `audio_pipewire` and `audio_pulse` modules already exist in `core.toml`.
- Without this: audio socket path falls back to `/run/user/1000/pipewire-0`, which breaks on non-1000 UIDs.

---

### Phase 3 — Observability `[PLANNED]`

**`cordon status` Command**
- New subcommand: show `system.toml` contents without running a scan.
- Display: each module name, verification status (`✅` / `⚠️`), source path, `when` category.
- Show `last_scan` timestamp and `cordon_version` from the file header.
- Useful for debugging "why isn't my module being mounted?" without running a full command.
- Wire in `cli.rs` and `main.rs` only — reads via `load_system_config()` from `config.rs`.

**strace Integration**
- Wrap bwrap with strace, capture blocked syscalls and paths.
- Parse strace output — show what the app tried to access but couldn't.
- Write a structured log of blocked paths after each run.

---

### Phase 4 — Polish & DX `[PLANNED]`

**`--quiet` / `--verbose` Flags**
- `--quiet`: suppress all Cordon output; only show sandboxed command output.
- `--verbose`: print every bwrap argument on its own line; show each mount as it is applied.
- Replace scattered `println!` calls with a thin logging helper that respects these flags.
- Both flags live in `Cli` (global, not per-subcommand).

**NixOS / Non-FHS Distro Support**
- Standard paths (`/usr`, `/bin`, `/lib`) do not exist on NixOS.
- Detect NixOS via `/etc/os-release` at scan time and adjust module `default_dir` values.
- Consider a `--nix-profile` flag that reads the active Nix profile path.
- The existing corrected-path prompt already handles this partially — this task makes it automatic.

**Per-Project Module Overrides in `cordon.toml`**
- Allow `require_optional = ["audio_pipewire"]` in `cordon.toml`.
- Auto-activates those optional modules without needing `--optional` every time.
- `executor.rs` merges this field into the `optional` vec before calling `apply_system_mounts()`.
- Add `require_optional` field to `UserConfig` struct in `config.rs`.

**Exit Code 126/127 Detection**
- bwrap exits 1 for both "command not found" and "not executable" — need to tell them apart.
- Capture bwrap stderr and inspect it to detect these cases.
- Re-emit exit 127 (not found) or 126 (not executable) accordingly.
- Constants already defined in `main.rs` — just not emitted yet.

**Test Suite**
- Unit tests for `resolve_env_vars()` and `resolve_dbus_socket()`.
- Unit tests for `scan_module_at()` covering: symlink, real directory, missing path.
- Integration test: run a simple `echo` command through the full sandbox stack.
- CI workflow (GitHub Actions) for `x86_64-unknown-linux-gnu`.

---

### Phase 5 — TUI `[PLANNED]`

- Interactive directory picker
- Toggle network / dry-run visually
- View mounts before running
- Edit `cordon.toml` via TUI

---

### Phase 6 — Profiles & Distribution `[PLANNED]`

- Built-in profiles (`nodejs`, `python`, `rust`) — pre-configured optional module sets
- GitHub Actions smoke tests
- Prebuilt binary releases
