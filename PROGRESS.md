# Cordon — Progress Tracking

All completed work and planned work lives here.

> **Command Reference**: See [COMMANDS.md](./COMMANDS.md) for a complete list of all current and planned commands with flags, examples, and phase numbers.

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
| GPU/DRI device node support | Added `dev-bind` to `MountEntry` and `scan_module_at()`, wiring in `gpu_dri` for GPU hardware acceleration |
| Audio socket resolution at scan time | `resolve_pipewire_socket()` and `resolve_pulse_socket()` in `env_resolver.rs` — reads `$PIPEWIRE_RUNTIME_DIR` / `$PULSE_RUNTIME_PATH`, falls back to `$XDG_RUNTIME_DIR` |
| Audio modules special-case in scanner | `scan_module_interactive()` calls respective audio socket resolvers before generic resolver |
| Home directory & env var secret protection | Removed implicit `$HOME` exposure, restricted environment to `LANG`, `LC_*`, `TERM`, `PATH`, `USER`, `HOME`; added `home_config` optional module |

---

### Phase 2.7 — Networking & Proxy ✅

| Feature | Notes |
|---------|-------|
| `--net=<mode>` flag | `disable` (default), `allow` (proxy), `full` (direct) |
| Native Rust Proxy | Multi-threaded HTTP/HTTPS domain-filtering proxy (no Python required) |
| `proxy.toml` support | Project-local and global domain allow-lists |
| CLI `--domain` flag | On-the-fly domain white-listing |
| Proxy log integration | Access logs printed to stderr with `[CORDON-PROXY]` prefix |

---

### Phase 2.7 — Project Profile (Default Run Mode) ✅

| Feature | Notes |
|---------|-------|
| `--net`, `--gui`, `--optional` profile persistence | Extended `UserConfig` with optional profile fields in `cordon.toml` |
| `cordon set` subcommand | e.g. `cordon set --net=allow`, `cordon set --gui`, `cordon set --optional dbus_session` |
| `cordon unset` subcommand | Safely removes profile fields without touching user mounts |
| Transparent CLI overrides | CLI flags always take precedence over `cordon.toml` defaults at runtime (implemented in `executor.rs`) |

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

### Documentation ✅

| Item | Notes |
|------|-------|
| `COMMANDS.md` created | Full command reference — all current commands with flags/examples + all planned commands with phase numbers |

---

## Pending

### Phase 2.6 — Scanner Completion `[COMPLETED]`

---

### Phase 2.8 — Named Profiles `[PLANNED]`

**`cordon profile` Subcommand**
- New subcommand: `cordon profile create <name> [--network] [--gui] [--optional <mod>]`
- Profiles stored in `~/.config/cordon/profiles.toml` as named blocks:
  ```toml
  [profile.NO_NET]
  network = false
  gui = false
  optional = []

  [profile.GUI_APP]
  network = true
  gui = true
  optional = ["audio_pipewire", "dbus_session"]
  ```
- `cordon run --profile <name> -- <cmd>` loads the named profile, then applies CLI flag overrides on top.
- `cordon profile list` — prints all defined profiles with their settings.
- `cordon profile delete <name>` — removes a profile.
- Add `ProfileConfig` struct to `config.rs`. Wire `profile` subcommand into `cli.rs` and `main.rs`.
- Profile resolution order (lowest → highest priority): built-in defaults → profile → `cordon.toml` → CLI flags.

---

### Phase 2.9 — Error Taxonomy ✅

| Feature | Notes |
|---------|-------|
| `CordonError` typed enum | `src/errors.rs` — 7 variants via `thiserror`; `DependencyMissing`, `CommandNotFound`, `PermissionDenied`, `ExecutionError`, `NamespaceError`, `ScanRequired`, `Internal` |
| Diagnostic error box | `main.rs` — ASCII box rendered on any `CordonError`, matches LION style |
| Smart failure hints | Exit 6 → "network disabled", 7 → "try --net=full", 35 → "SSL error", 126/127 detection |
| Structured exit codes | `CommandNotFound` → 127, `PermissionDenied` → 126, `DependencyMissing` → 125 automatically |
| Binary find + exec check | `executor.rs` — disambiguates `CommandNotFound` vs `PermissionDenied` on exit 1/126/127 |
| Log path in error box | Always shows `~/.config/cordon/logs/last-run.log` hint |

---

### Phase 2.10 — Structured Logging ✅

| Feature | Notes |
|---------|-------|
| `logger.rs` — dual-sink tracing | `tracing` + `tracing-subscriber` + `tracing-appender` |
| File sink | Full `TRACE` always written to `~/.config/cordon/logs/last-run.log` (non-blocking) |
| Stderr sink | `INFO` level by default; `DEBUG` when `--debug` is passed |
| `--debug` flag on `cordon run` | Activates verbose DEBUG output on stderr |
| All `println!`/`eprintln!` replaced | `executor.rs`, `builder.rs`, `mounts.rs` now use `tracing::info!`/`warn!`/`error!` |
| Extended proxy env vars | `npm_config_proxy`, `npm_config_https_proxy`, `PIP_PROXY` now set alongside standard vars |

---

### Phase 3 — Observability (Partial) ✅

| Feature | Notes |
|---------|-------|
| `cordon check` | 7-point health check: bwrap install, userns creation, AppArmor flag, system.toml validity, core/network/GUI module state. Colored OK/WARN/FAIL table + summary line |
| `cordon list` | Lists all system mounts (system.toml) with verified/unverified indicators grouped by `when` (always/network/gui/optional), plus project mounts (cordon.toml) with path-exists checks |
| `cordon status` | Shows system.toml contents without scanning — module name, verified (✅/⚠️), bind type, `when` category, source path; header shows `last_scan` and `cordon_version`; per-category breakdown footer |
| Smart error suggestions | Unknown subcommand → Levenshtein nearest-match → "did you mean cordon X?"; missing required arg → full command syntax. Implemented in `src/suggestions.rs`, hooked via `Cli::try_parse()` in `main.rs`. Covers all present and future commands. |

---

### Phase 3 — Observability (Remaining) `[PLANNED]`

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
