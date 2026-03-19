# Cordon — Module & File Reference

> Developer guide. One read = full mental model of the codebase.

---

## Repository Layout

```
Cordon/
├── Cargo.toml              # Rust manifest + dependencies
├── config/
│   └── core.toml           # Module blueprint (compiled into binary)
├── src/
│   ├── main.rs             # Entry point — routes CLI to modules, nothing else
│   ├── cli.rs              # Argument structs (clap). No logic.
│   ├── config.rs           # Data types + file I/O for all three config layers
│   ├── scanner/            # System scanner — detects paths, writes system.toml
│   │   ├── mod.rs
│   │   ├── env_resolver.rs
│   │   ├── full_scan.rs
│   ├── commands/           # Standalone subcommand implementations
│   │   ├── check.rs
│   │   ├── list.rs
│   │   └── status.rs       # ← NEW: cordon status (shows system.toml)
│   └── suggestions.rs      # ← NEW: smart error suggestions & synopses
├── COMMANDS.md             # ← NEW: full command reference & future plans
├── MODULE_INFO.md          # ← you are here
├── README.md               # User-facing docs (what the product does)
├── PROGRESS.md             # All completed and planned work
├── SCANNER_LOGIC.md        # Internal scanner design and architecture
└── FEATURE_LIST.md         # Redirect → PROGRESS.md
```

---

## Config Layer (the three-file system)

Before reading individual files, understand the three configs that flow through the whole codebase:

| File | Lives at | Who writes it | Who reads it |
|------|----------|---------------|--------------|
| `core.toml` | compiled into binary | developer (rebuild required) | scanner at scan time |
| `system.toml` | `~/.config/cordon/` | `full_scan()` only | `integrity_check()`, bwrap |
| `cordon.toml` | project directory (walks up) | `cordon add` / developer | sandbox mounts loop |

**The rule:** bwrap never reads env vars or hardcoded paths. Everything it needs is resolved at scan time and stored in `system.toml`.

---

## `config/core.toml`

The **blueprint**. Describes every possible module Cordon knows about.

- Compiled into the binary via `include_str!()` — cannot be modified at runtime.
- Changing it requires a rebuild. This is intentional (tamper-proof).
- Each `[[module]]` entry has: `name`, `description`, `default_dir`, `required_files`, `functionality`, `mode`, `when`, `required`.
- `when` controls which activation flag exposes a module: `always | network | gui | optional`.
- `required = true` means the sandbox hard-fails if the module is unverified.

**Dev note:** if you add a new module here, the scanner will automatically pick it up and
ask the user about it during the next `cordon scan`. No code change needed.

---

## `src/main.rs`

**Pure router.** Contains zero business logic.

- Parses CLI via clap, then does one `match` → dispatches to `sandbox::run_sandboxed`, `scanner::full_scan`, or `config::add_user_mount`.
**Dev note:** if you add a new subcommand, you need to touch `cli.rs` (variant), `main.rs` (dispatch), and `suggestions.rs` (synopsis & known commands list).

---

## `src/suggestions.rs`

**Smart error handling** using Levenshtein distance.

- `KNOWN_COMMANDS` — list of all implemented subcommand names.
- `command_synopsis()` — returns a one-line usage string for each command.
- `closest_command()` — finds the best match for a typo within 3 edits.
- `print_unknown_command_error()` / `print_missing_arg_error()` — formatted, actionable error printers called from `main.rs`.

---

## `src/cli.rs`

**Argument structs only.** No logic lives here.

- `Cli` is the top-level clap `Parser`.
- `Commands` is the `Subcommand` enum: `Run { ... }`, `Scan {}`, `Add { ... }`, `Set { ... }`, `Unset { ... }`.
- `Run` has: `cmd`, `network`, `dry_run`, `gui`, `optional`.
- `#[arg(last = true)]` on `cmd` is what makes `cordon run -- <cmd>` work — everything after `--` goes to the sandboxed process.

---

## `src/config.rs`

**Data types + file I/O** for all three config layers. No scanning logic. No bwrap logic.

### Structs

| Struct | Maps to | Description |
|--------|---------|-------------|
| `CoreModule` | one `[[module]]` in `core.toml` | Blueprint entry |
| `CoreConfig` | entire `core.toml` | Container for all `CoreModule`s |
| `MountEntry` | one `[[mount]]` in `system.toml` | Verified path record written by scanner |
| `SystemConfig` | entire `system.toml` | Contains `last_scan`, `cordon_version`, vec of `MountEntry` |
| `UserMount` | one `[[mount]]` in `cordon.toml` | User-defined extra mount |
| `UserConfig` | entire `cordon.toml` | Container for all `UserMount`s |

### Key fields on `MountEntry`
- `bind_type`: `"ro-bind"` / `"bind"` / `"symlink"` — maps directly to a bwrap flag.
- `verified`: `true` means all `required_files` were found at scan time. `false` = path exists but file check failed, or path was missing entirely.
- `when`: same values as `core.toml` — controls whether this mount is applied.

### Functions

| Function | Does |
|----------|------|
| `get_config_dir()` | Returns `~/.config/cordon/`, creates it if missing |
| `load_system_config()` | Reads + parses `system.toml` (utility; not called in main flow) |
| `save_system_config()` | Writes `system.toml` with `fd-lock` write lock to prevent concurrent corruption |
| `find_user_config()` | Walks up the directory tree from cwd looking for `cordon.toml`, stops at `/` |
| `add_user_mount()` | Appends a `UserMount` to `cordon.toml` in cwd, creates file if absent |
| `set_profile_field()` | Sets a `network`, `gui`, or `optional` profile default in `cordon.toml` |
| `unset_profile_field()` | Removes a profile default flag from `cordon.toml` |

**Dev note:** `save_system_config` uses `fd-lock` because `cordon scan` could be Ctrl-C'd mid-write. Any concurrent `cordon run` would then see a partial file. The lock prevents that.

---

## `src/scanner/`

Owns all path-discovery logic. Has **two public functions**: `full_scan()` and `integrity_check()`. Nothing else should call into this module.

### `mod.rs`

Declares the sub-modules, re-exports `full_scan` and `integrity_check`, and defines:
```rust
pub(crate) const CORE_TOML: &str = include_str!("../../config/core.toml");
```
This is the single point where `core.toml` enters the binary.

---

### `env_resolver.rs`

**Two pure functions** for resolving runtime paths at scan time.

#### `resolve_env_vars(path: &str) -> String`
Replaces the `/run/user/1000` placeholder in `core.toml` paths with the real `$XDG_RUNTIME_DIR` (e.g. `/run/user/1001`). Called for every module whose `default_dir` contains that placeholder.

**Why this exists:** UIDs vary per user. Hardcoding `1000` would break on any system where the user's UID is different.

#### `resolve_dbus_socket() -> Option<String>`
Dedicated resolver for the D-Bus session socket. Tries:
1. `$DBUS_SESSION_BUS_ADDRESS` — parses `unix:path=/run/user/1000/bus`, strips the prefix and any `,guid=…` suffix, checks the file exists.
2. `$XDG_RUNTIME_DIR/bus` — conventional fallback.

Returns `None` if not found. The `dbus_session` module in `module_scan.rs` calls this directly instead of the generic resolver.

---

### `module_scan.rs`

**Two complementary functions** — one interactive, one pure.

#### `scan_module_interactive(module: &CoreModule) -> Result<Option<MountEntry>>`
Called during `full_scan`. Handles special cases before delegating to `scan_module_at`:
- `dbus_session` → calls `resolve_dbus_socket()` first.
- Required module not found → prompts user for a corrected path (handles NixOS / non-FHS layouts).
- Optional module not found → passes the missing path straight to `scan_module_at`, which records it as `verified = false`.

#### `scan_module_at(module: &CoreModule, dir: &str) -> Result<Option<MountEntry>>`
Pure scan at a specific known path. No prompts. Does:
1. Path missing → returns `MountEntry` with `verified = false`.
2. Path is a symlink → reads the raw link target, stores it as `src`, sets `bind_type = "symlink"`.
3. Path is a real dir → verifies `required_files` exist inside it, sets `bind_type` from `mode`.

**Why store the raw symlink target?** bwrap's `--symlink <target> <dest>` recreates the symlink inside the sandbox. If we stored the resolved path instead, bwrap would try to bind-mount a real directory at the symlink location — which is wrong on merged-usr distros.

---

### `full_scan.rs`

**Interactive, four-phase scanner. The only function that writes `system.toml`.**

Phases:
1. **Always** — scans mandatory modules automatically, no user choice.
2. **Network** — single yes/no: "Include network support?"
3. **GUI** — single yes/no: "Include GUI support?"
4. **Optional** — each module shown with description + `functionality`, individually asked.

Flow: collect `Vec<MountEntry>` → build `SystemConfig` → call `save_system_config()`.

**When it runs:** on first `cordon run` (system.toml missing), on `cordon scan`, or when `integrity_check` detects a problem it cannot self-heal.

---

### `integrity.rs`

**Non-interactive, 7-step pre-flight check. Runs before every `cordon run`.**

Returns `SystemConfig` on success. Errors are fatal — `executor.rs` propagates them to `main.rs`.

| Step | Description | On failure |
|------|-------------|------------|
| 1 | Parse `system.toml` | Trigger `full_scan` |
| 2 | Version check (binary vs `system.toml`) | Trigger `full_scan` |
| 3 | Foreign entry check (unknown module name) | Hard block — security gate |
| 4 | File existence for all verified mounts | Trigger `full_scan` |
| 5 | Required `always` modules must be verified | Hard block |
| 6 | `--network` gate: required network modules verified? | Hard block |
| 7 | `--gui` gate: required GUI modules verified? | Hard block |

**Dev note:** Steps 1, 2, and 4 auto-heal by triggering `full_scan`. Steps 3, 5, 6, and 7 are hard blocks because there is no safe automatic recovery — they require explicit user action (`cordon scan`).

---

## `src/sandbox/`

Owns bwrap invocation. **Reads config, never writes it.** All paths come from `system.toml` and `cordon.toml`.

### `mod.rs`

Declares sub-modules and re-exports `run_sandboxed`. Documents the exit-code contract.

---

### `builder.rs`

**Builds the base bwrap `Command` object.**

#### `build_bwrap(project_path, network, dry_run) -> Command`
Sets up namespace isolation flags (`--unshare-user`, `--unshare-ipc`, `--unshare-pid`, etc.), pseudo-filesystems (`--tmpfs /tmp`, `--proc /proc`, `--dev /dev`), and the project directory writable bind (`--bind <project> <project>`). Conditionally adds `--unshare-net`.

#### `apply_environment(bwrap, gui)`
Adds `--setenv` args for safe env vars: `HOME`, `USER`, `LOGNAME`, `LANG`, `LC_ALL`, `PATH`, `XDG_*`. Adds `DISPLAY` and `WAYLAND_DISPLAY` only when `--gui` is active.

**Dev note:** `DBUS_SESSION_BUS_ADDRESS` is intentionally NOT forwarded here. The D-Bus socket is bind-mounted directly (via `system.toml`), which is safer than forwarding the env var and hoping the sandboxed process can reach the path.

---

### `mounts.rs`

**Applies mount arguments to the bwrap command** from both config sources.

#### `apply_system_mounts(bwrap, system_config, network, gui, optional)`
Iterates `system_config.mounts`. For each entry:
1. Filter by `when`: skip network mounts without `--network`, GUI mounts without `--gui`, optional mounts not in `optional` list.
2. Skip `verified = false` entries (warn only if user explicitly requested it via `--optional`).
3. Call `bwrap.arg("--<bind_type>").arg(src).arg(dest)`.

#### `apply_user_mounts(bwrap, dry_run)`
Calls `find_user_config()`. If `cordon.toml` found:
- Dry-run: includes all mounts silently (so the printed bwrap command is complete).
- Normal run: prompts user `[Enter=yes / N=no / D=show paths]` before applying.
- Errors reading `cordon.toml` surface as warnings, not fatal failures.

---

### `executor.rs`

**Orchestrates the full `cordon run` flow.** This is the integration point — it calls everything else.

1. Check `bwrap --version` works (exit 125 if not).
2. Call `integrity_check(network, gui)` → get `SystemConfig`.
3. Call `build_bwrap(project_path, network, dry_run)`.
4. Call `apply_system_mounts(...)`.
5. Call `apply_user_mounts(...)`.
6. Bind `src/` as read-only if it exists in the project.
7. Call `apply_environment(...)`.
8. Add `--chdir <project>` and `-- <cmd>`.
9. Dry-run: print the full command, return.
10. Normal: `bwrap.status()` → on non-zero exit, encode as `bail!("exit code: N")`.

**Dev note on exit code encoding:** `executor.rs` cannot call `std::process::exit()` directly because it returns `Result`. Instead it encodes the child's exit code into an error string `"exit code: N"`. `main.rs` calls `extract_exit_code()` which parses this back out and calls `std::process::exit(N)`. This lets the full chain (sandbox → cordon → shell) see the real child exit code.

---

## Data Flow Diagram

```
cordon run -- npm install
       │
       ▼
   main.rs  ──── dispatch ────►  executor::run_sandboxed()
                                        │
                              ┌─────────┴──────────┐
                              ▼                    ▼
                     integrity_check()      build_bwrap()
                              │                    │
                     reads system.toml    apply_system_mounts()
                              │            (itereates MountEntry)
                              │                    │
                     may call full_scan()   apply_user_mounts()
                              │            (reads cordon.toml)
                              │                    │
                              └─────────┬──────────┘
                                        ▼
                                bwrap.status()  ──►  child process
                                        │
                              encode exit code
                                        │
                                        ▼
                                   main.rs
                               extract_exit_code()
                                        │
                               std::process::exit(N)
```

---

## Key Invariants

1. **Only `full_scan()` writes `system.toml`.** Nothing else does.
2. **`integrity_check()` never writes anything under normal operation.** It may call `full_scan()` on errors.
3. **No hardcoded paths in the sandbox module.** All paths come from `system.toml` and `cordon.toml`.
4. **`core.toml` is tamper-proof at runtime.** It lives in the binary. A sandboxed process cannot affect it.
5. **`verified = false` mounts are never passed to bwrap.** They are skipped unconditionally.
6. **Exit codes are forwarded exactly.** The sandboxed process's exit code reaches the calling shell unmodified.
