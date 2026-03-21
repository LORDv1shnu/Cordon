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

### Phase 2.8 — Named Profiles ✅

| Feature | Notes |
|---------|-------|
| `cordon profile create <name>` | Create a named profile with `--net`, `--gui`, `--optional` |
| `cordon profile list` | Show all saved profiles in a table |
| `cordon profile delete <name>` | Remove a named profile |
| `cordon profile show <name>` | Dump a single profile's TOML representation |
| `cordon run --profile <name>` | Apply named profile (overridden by CLI flags) |
| `profiles.toml` layer | Stored at `~/.config/cordon/profiles.toml` |
| Built-in profiles | `python`, `node`, `rust`, `gui-app` seamlessly resolved |

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
| `cordon run --trace` | Strace wrapper capturing denied openat/access with pretty report |
| `cordon add --from-trace` | Batch import missing paths from a `last-trace.log` |
| `cordon log` | Read and filter `last-run.log` (`--last <N>`, `--errors`) |

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

### Phase 4 — Polish & Developer Experience ✅

| Feature | Notes |
|---------|-------|
| `--quiet` / `--verbose` flags | Suppress output / show full bwrap args |
| `cordon init` | Auto-detects project, guides scaffold of `cordon.toml` |
| Exit code 0 for `--help`/`--version` | Fixed clap bug |
| NixOS / Non-FHS support | Scans `/run/current-system/sw` and mounts `/nix/store` |
| `cordon doctor` | Deep check with fix suggestions for each failure |

---

### Phase 4.5 — Resource Limits `[PLANNED]`

Right now cordon only restricts *filesystem visibility*. These features extend sandboxing to *resource consumption*.

**Memory Limit (`--mem <SIZE>`)**
- Apply a cgroup v2 memory limit to the sandboxed process.
- Example: `cordon run --mem=512m -- npm install`
- Uses `systemd-run --scope` or writes directly to the cgroup hierarchy.
- On OOM: cordon prints a clear error (not a kernel OOM panic), exits 125.

**CPU Limit (`--cpu <N>`)**
- Restrict the sandbox to N logical CPUs (or a fraction via `--cpu=0.5`).
- Uses `cgroup v2 cpu.max` or `cpuset`.
- Prevents data-exfil via CPU side-channels or runaway compile jobs from eating the machine.

**Time Limit (`--timeout <SECS>`)**
- Kill the sandboxed process after N seconds.
- Cordon handles the timer and prints `[CORDON] sandbox timed out after Ns`.
- Exits with code 124 (matching the `timeout(1)` convention).

**`--pid-limit <N>`**
- Limit the number of processes the sandbox can spawn via cgroup `pids.max`.
- Prevents fork bombs inside the sandbox.

---

### Phase 5 — Syscall Filtering (seccomp) `[PLANNED]`

Adding a seccomp filter layer makes cordon a genuinely layered sandbox:

```
filesystem isolation (bwrap namespaces)   ← already done
 + network isolation (proxy / unshare)    ← already done
 + syscall filtering (seccomp)            ← this phase
 + resource limits (cgroups)              ← Phase 4.5
```

**`--seccomp <PRESET>` Flag**
- `basic` — block a minimal set of dangerous syscalls: `ptrace`, `process_vm_readv`, `userfaultfd`, `perf_event_open`, `kexec_load`.
- `strict` — block everything not in a known-good allowlist (baseline: what Chrome uses).
- `none` — disable seccomp entirely (for debugging).
- Custom policy file: `--seccomp-file path/to/policy.bpf`.

**How it works:**
- Generate a BPF program from the preset and pass it to bwrap via `--seccomp <fd>`.
- Policy is compiled at runtime from a human-readable TOML list in `core.toml`.
- Deny action: `ENOSYS` (cleaner than SIGKILL — app gets a sensible error).

**`cordon syscalls` Subcommand**
- Lists the syscalls blocked by each preset in a readable table.
- Example: `cordon syscalls --preset strict`

---

### Phase 5.5 — Reproducible Sandbox Specs `[PLANNED]`

**`cordon.lock` File**

The problem: `cordon.toml` says *what* to mount but not *what version* of the system modules were used. After a distro upgrade, the sandbox might silently behave differently.

- After each successful run, write `cordon.lock` with:
  - SHA-256 of every mount path used.
  - Cordon version, scan timestamp.
- `cordon run` checks the lock: if a mount path has changed (different inode / hash), warn the user before running.
- `cordon lock update` — regenerate the lock after an intentional upgrade.
- `cordon lock verify` — standalone check (useful in CI).

**`cordon export` / `cordon import`**
- `cordon export > sandbox-spec.json` — dump the full resolved mount list as JSON.
- `cordon import sandbox-spec.json` — write a `cordon.toml` from an exported spec.
- Makes sandbox configs portable and shareable between developers on the same distro.

---

### Phase 6 — Shell & Editor Integration `[PLANNED]`

**Shell Completions**
- `cordon completions bash` / `zsh` / `fish` — generate and print shell completion scripts.
- Completions for: subcommand names, `--net` values, `--optional` module names, profile names.
- Install instructions: `cordon completions zsh > ~/.zfunc/_cordon`.

**`cordon wrap <cmd>`**
- Creates a tiny shell wrapper script `~/.local/bin/<cmd>` that calls `cordon run -- <cmd> "$@"`.
- Lets you transparently sandbox any tool without changing your workflow:
  ```bash
  cordon wrap node    # now `node` always runs sandboxed
  cordon wrap pip     # same for pip
  ```
- `cordon wrap --show <cmd>` — print the wrapper script before installing.
- `cordon unwrap <cmd>` — remove the wrapper.

**Man Page**
- Auto-generate `cordon.1` from clap definitions via `clap_mangen`.
- Install to `~/.local/share/man/man1/cordon.1`.

---

### Phase 7 — TUI `[PLANNED]`

A lightweight terminal UI for interactive sandbox configuration (using `ratatui`).

- `cordon tui` — opens the TUI from any project directory.
- **Mount panel:** directory tree picker; toggle ro/rw; shows cordon.toml entries live.
- **Profile panel:** toggle network mode, GUI, optional modules via checkboxes.
- **Preview panel:** shows the resolved bwrap command that would run.
- **Run panel:** execute the sandboxed command from within the TUI; stream output live.
- **Log panel:** tail `last-run.log` in a scrollable pane.

---

### Phase 8 — Distribution & CI `[PLANNED]`

**GitHub Actions CI**
- `cargo build --release` on `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`.
- Run `test.sh` (the CLI regression suite) in CI on every PR.
- Run `cargo test` (unit tests) in CI.
- Lint: `cargo clippy -- -D warnings`.

**Prebuilt Binary Releases**
- GitHub Releases with prebuilt `cordon-linux-x86_64` and `cordon-linux-aarch64` binaries.
- Install one-liner: `curl -fsSL https://... | sh`.
- Checksums and signatures (via `minisign` or `cosign`).

**Package Manager**
- AUR package (`cordon-bin`) for Arch Linux users.
- Nix flake / `default.nix` for NixOS users.
- Homebrew tap (Linux only) for cross-distro reach.

**`cordon install` Subcommand**
- One-time system configuration: writes an AppArmor profile that allows cordon to use user namespaces without restriction.
- Currently the fix is manual (`sudo sysctl ...`); this should be automated.
- Example: `sudo cordon install` → writes `/etc/apparmor.d/cordon` and reloads.
