# Cordon — Progress Tracking

> **Reading order:** [README.md](README.md) → [COMMANDS.md](COMMANDS.md) → [SCANNER_LOGIC.md](SCANNER_LOGIC.md) → [MODULE_INFO.md](MODULE_INFO.md) → **PROGRESS.md**
>
> All completed work and planned work lives here. For the command reference, see [COMMANDS.md](COMMANDS.md).

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
| Clean exit code forwarding | child exit code forwarded exactly |
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
| `integrity_check()` — 7-step pre-flight | see [SCANNER_LOGIC.md](SCANNER_LOGIC.md) |
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

---

### Phase 2.5 — Runtime Environment Support ✅

| Feature | Notes |
|---------|-------|
| `--optional <module>` flag | activates optional modules at runtime; warns if unverified |
| `cordon add <path> --mode <rw\|ro>` | appends to `cordon.toml`; src=dest=canonical absolute path |
| `cordon remove <path>` | removes entry from `cordon.toml` by canonical path |
| `cordon edit` | opens `cordon.toml` in `$EDITOR` |
| Safe env var passthrough | `HOME`, `USER`, `LOGNAME`, `LANG`, `LC_ALL`, `PATH`, `XDG_*` |
| GUI env vars | `DISPLAY`, `WAYLAND_DISPLAY` forwarded only with `--gui` |
| `cordon.toml` confirmation prompt | `Enter=yes / N=no / D=show paths`; never applied silently |
| Dry-run includes `cordon.toml` mounts | so printed command is complete |

---

### Phase 2.6 — Scanner Completion ✅

| Feature | Notes |
|---------|-------|
| D-Bus socket resolution at scan time | `resolve_dbus_socket()` in `env_resolver.rs` |
| `dbus_session` special-case in scanner | calls `resolve_dbus_socket()` before generic resolver |
| GPU/DRI device node support | Added `dev-bind` to `MountEntry`; wired in `gpu_dri` |
| Audio socket resolution at scan time | `resolve_pipewire_socket()` and `resolve_pulse_socket()` |
| Audio modules special-case in scanner | calls respective audio socket resolvers |
| Home directory & env var secret protection | Removed implicit `$HOME` exposure; added `home_config` optional module |

---

### Phase 2.7 — Project Profile (Default Run Mode) ✅

| Feature | Notes |
|---------|-------|
| `network`, `gui`, `optional` profile fields in `cordon.toml` | Extended `UserConfig` with `Option<T>` fields — backward compatible |
| `cordon set` subcommand | e.g. `cordon set --net=allow --gui --optional dbus_session` |
| `cordon unset` subcommand | Removes profile fields without touching mounts |
| Transparent CLI merge in `executor.rs` | CLI flags always override `cordon.toml` profile defaults |
| Unit tests for profile serialization | 5 tests in `config.rs` covering all field variants |

---

### Phase 2.9 — Error Taxonomy ✅

| Feature | Notes |
|---------|-------|
| `CordonError` typed enum | `src/errors.rs` — 7 variants via `thiserror` |
| Diagnostic error box | `main.rs` — ASCII box rendered on any `CordonError` |
| Smart failure hints | Exit 6 → "network disabled", 7 → "try --net=full", etc. |
| Structured exit codes | `CommandNotFound` → 127, `PermissionDenied` → 126, etc. automatically |
| Binary find + exec check | `executor.rs` — disambiguates `CommandNotFound` vs `PermissionDenied` |
| Log path in error box | Always shows `~/.config/cordon/logs/last-run.log` hint |

---

### Phase 2.10 — Structured Logging ✅

| Feature | Notes |
|---------|-------|
| `logger.rs` — dual-sink tracing | `tracing` + `tracing-subscriber` + `tracing-appender` |
| File sink | Full `TRACE` always written to `~/.config/cordon/logs/last-run.log` |
| Stderr sink | `INFO` level by default; `DEBUG` when `--debug` is passed |
| `--debug` flag on `cordon run` | Activates verbose DEBUG output on stderr |
| All `println!`/`eprintln!` replaced | `executor.rs`, `builder.rs`, `mounts.rs` use `tracing` macros |
| Extended proxy env vars | `npm_config_proxy`, `npm_config_https_proxy`, `PIP_PROXY` |

---

### Code Quality Pass ✅

| Area | Change |
|------|--------|
| `config.rs` `add_user_mount` | Fixed broken dest path |
| `config.rs` `find_user_config` | Added `with_context` error annotations; fixed double `/` stop condition |
| `scanner/env_resolver.rs` | Removed stale `#[allow(clippy::collapsible_if)]`; simplified guard order |
| `scanner/integrity.rs` | CORE_TOML parsed once at top; added Step 5 required-always gate |
| `sandbox/mounts.rs` | Removed dead double `verified` check |
| `sandbox/executor.rs` | bwrap check uses `bwrap --version` instead of `which bwrap` |

---

### Phase 3 — Observability ✅

| Feature | Notes |
|---------|-------|
| `cordon check` | 7-point health check with colored OK/WARN/FAIL table |
| `cordon list` | Lists all system + project mounts with verification indicators |
| `cordon status` | Shows `system.toml` contents without scanning |
| Smart error suggestions | Levenshtein nearest-match in `src/suggestions.rs` |

---

### Documentation ✅

| Item | Notes |
|------|-------|
| `COMMANDS.md` | Full command reference with reading-order nav |
| `SCANNER_LOGIC.md` | Internal scanner and architecture design notes |
| `MODULE_INFO.md` | Per-file developer breakdown of every source file |
| `PROGRESS.md` | This file — complete phase history + roadmap |

---

## Pending

### Phase 2.8 — Named Profiles `[PLANNED]`

**`cordon profile` Subcommand**

- `cordon profile create <name> [--net <PROFILE>] [--gui] [--optional <mod>]`
- Profiles stored in `~/.config/cordon/profiles.toml`
- `cordon run --profile <name> -- <cmd>` loads the profile, then CLI flags override
- `cordon profile list` / `cordon profile delete <name>`
- Resolution order: built-in defaults → named profile → `cordon.toml` → CLI flags

---

### Phase 3 — Observability (Remaining) `[PLANNED]`

**strace Integration**
- Wrap bwrap with strace, capture blocked syscalls and paths.
- Parse strace output — show what the app tried to access but couldn't.
- Write a structured log of blocked paths after each run.

---

### Phase 4 — Polish & DX `[PLANNED]`

**`--quiet` / `--verbose` Flags**
- `--quiet`: suppress all Cordon output.
- `--verbose`: print every bwrap argument; show each mount as it is applied.

**NixOS / Non-FHS Distro Support**
- Auto-detect NixOS via `/etc/os-release` and adjust module `default_dir` values.

**Test Suite**
- Unit tests for `resolve_env_vars()` and `resolve_dbus_socket()`.
- Unit tests for `scan_module_at()` covering symlink, real dir, missing path.
- Integration test: run a simple `echo` through the full sandbox stack.
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
