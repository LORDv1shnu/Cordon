# Cordon — Module & File Reference

> **Reading order:** [README.md](README.md) → [COMMANDS.md](COMMANDS.md) → [SCANNER_LOGIC.md](SCANNER_LOGIC.md) → **MODULE_INFO.md** → [PROGRESS.md](PROGRESS.md)
>
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
│   ├── errors.rs           # CordonError typed enum (thiserror)
│   ├── logger.rs           # Dual-sink tracing logger (stderr + log file)
│   ├── suggestions.rs      # Smart "did you mean?" suggestions & synopses
│   ├── commands/           # Standalone subcommand implementations
│   │   ├── check.rs        # cordon check
│   │   ├── list.rs         # cordon list
│   │   ├── profile.rs      # cordon profile (create, list, delete, show)
│   │   └── status.rs       # cordon status
│   ├── scanner/            # System scanner — detects paths, writes system.toml
│   │   ├── mod.rs
│   │   ├── env_resolver.rs # XDG_RUNTIME_DIR + D-Bus + audio socket resolution
│   │   ├── full_scan.rs    # Interactive 4-phase scanner
│   │   ├── integrity.rs    # 7-step pre-flight check
│   │   └── module_scan.rs  # Per-module scan logic
│   └── sandbox/            # bwrap invocation — reads config, never writes it
│       ├── mod.rs
│       ├── builder.rs      # Base bwrap command + env var passthrough
│       ├── executor.rs     # Orchestrates the full cordon run flow
│       ├── mounts.rs       # Applies system + user mounts to bwrap command
│       ├── network.rs      # NetworkMode enum
│       └── proxy.rs        # Native Rust domain-filtering HTTP/HTTPS proxy
├── COMMANDS.md             # Full command reference & future plans
├── MODULE_INFO.md          # ← you are here
├── PROGRESS.md             # All completed and planned work
├── README.md               # User-facing docs
├── SCANNER_LOGIC.md        # Internal scanner design and architecture
```

---

## Config Layer (the four-file system)

| File | Lives at | Who writes it | Who reads it |
|------|----------|---------------|--------------|
| `core.toml` | compiled into binary | developer (rebuild required) | scanner at scan time |
| `system.toml` | `~/.config/cordon/` | `full_scan()` only | `integrity_check()`, bwrap |
| `profiles.toml` | `~/.config/cordon/` | `cordon profile create` | executor.rs profile merge |
| `cordon.toml` | project directory (walks up) | `cordon add` / `cordon set` / developer | sandbox mounts loop + executor.rs profile merge |

**The rule:** bwrap never reads env vars or hardcoded paths. Everything it needs is resolved at scan time and stored in `system.toml`.

---

## `config/core.toml`

The **blueprint**. Describes every possible module Cordon knows about.

- Compiled into the binary via `include_str!()` — cannot be modified at runtime.
- Each `[[module]]` entry has: `name`, `description`, `default_dir`, `required_files`, `functionality`, `mode`, `when`, `required`.
- `when` controls which activation flag exposes a module: `always | network | gui | optional`.
- `required = true` means the sandbox hard-fails if the module is unverified.

**Dev note:** adding a new module here is the only change needed — the scanner will automatically pick it up.

---

## `src/main.rs`

**Pure router.** Contains zero business logic.

Parses CLI via clap, then dispatches to the right module. If you add a new subcommand, touch `cli.rs` (variant), `main.rs` (dispatch), and `suggestions.rs` (synopsis & known commands list).

---

## `src/suggestions.rs`

**Smart error handling** using Levenshtein distance.

- `KNOWN_COMMANDS` — list of all implemented subcommand names.
- `command_synopsis()` — returns a one-line usage string for each command.
- `closest_command()` — finds the best match for a typo within 3 edits.
- `print_unknown_command_error()` / `print_missing_arg_error()` — formatted, actionable error printers.

---

## `src/cli.rs`

**Argument structs only.** No logic lives here.

- `Cli` is the top-level clap `Parser`.
- `Commands` is the `Subcommand` enum: `Run { ... }`, `Scan {}`, `Add { ... }`, `Remove { ... }`, `Edit {}`, `Set { ... }`, `Unset { ... }`, `Check`, `List`, `Status`, `Profile { ... }`.
- `Run` has: `cmd`, `net`, `domains`, `dry_run`, `gui`, `optional`, `debug`.
- `#[arg(last = true)]` on `cmd` is what makes `cordon run -- <cmd>` work.

---

## `src/config.rs`

**Data types + file I/O** for all three config layers. No scanning logic. No bwrap logic.

### Structs

| Struct | Maps to | Description |
|--------|---------|-------------|
| `CoreModule` | `[[module]]` in `core.toml` | Blueprint entry |
| `CoreConfig` | entire `core.toml` | Container for all `CoreModule`s |
| `MountEntry` | `[[mount]]` in `system.toml` | Verified path record written by scanner |
| `SystemConfig` | entire `system.toml` | Contains `last_scan`, `cordon_version`, vec of `MountEntry` |
| `NamedProfile` | `[[profile]]` in `profiles.toml` | Reusable global configuration profile |
| `ProfilesConfig` | entire `profiles.toml`| Contains list of `NamedProfile` |
| `UserMount` | `[[mount]]` in `cordon.toml` | User-defined extra mount |
| `UserConfig` | entire `cordon.toml` | Mounts + profile defaults (`network`, `gui`, `optional`) |

### Key fields on `MountEntry`
- `bind_type`: `"ro-bind"` / `"bind"` / `"symlink"` — maps directly to a bwrap flag.
- `verified`: `true` means all `required_files` were found at scan time.
- `when`: same values as `core.toml` — controls whether this mount is applied.

### Key fields on `UserConfig` (Phase 2.7)
- `network: Option<String>` — default network profile (`"disable"` / `"allow"` / `"full"`).
- `gui: Option<bool>` — default GUI flag.
- `optional: Option<Vec<String>>` — default optional modules.

All three are `Option<T>` so existing `cordon.toml` files without them continue to parse correctly.

### Functions

| Function | Does |
|----------|------|
| `get_config_dir()` | Returns `~/.config/cordon/`, creates it if missing |
| `load_system_config()` | Reads + parses `system.toml` |
| `save_system_config()` | Writes `system.toml` with `fd-lock` write lock |
| `get_profiles_path()` | Returns `~/.config/cordon/profiles.toml` |
| `load_profiles()` | Reads + parses `profiles.toml` |
| `save_profiles()` | Writes `profiles.toml` |
| `find_user_config()` | Walks up the directory tree from cwd looking for `cordon.toml` |
| `add_user_mount()` | Appends a `UserMount` to `cordon.toml`, creates file if absent |
| `remove_user_mount()` | Removes a `UserMount` from `cordon.toml` by canonical path |
| `edit_user_config()` | Opens `cordon.toml` in `$EDITOR` |
| `set_profile_field()` | Sets a `network`, `gui`, or `optional` profile default in `cordon.toml` |
| `unset_profile_field()` | Removes a profile default flag from `cordon.toml` |

---

## `src/scanner/`

Owns all path-discovery logic. Has **two public functions**: `full_scan()` and `integrity_check()`.

### `env_resolver.rs`

- `resolve_env_vars(path)` — replaces `$XDG_RUNTIME_DIR` placeholder in `core.toml` paths.
- `resolve_dbus_socket()` — parses `$DBUS_SESSION_BUS_ADDRESS`, falls back to `$XDG_RUNTIME_DIR/bus`.
- `resolve_pipewire_socket()` / `resolve_pulse_socket()` — audio socket path resolution.

### `module_scan.rs`

- `scan_module_interactive()` — called during `full_scan`; handles special cases (D-Bus, missing required paths).
- `scan_module_at()` — pure scan at a specific path. Detects symlinks vs real dirs, checks `required_files`.

### `full_scan.rs`

Interactive, four-phase scanner. The only function that writes `system.toml`. See [SCANNER_LOGIC.md](SCANNER_LOGIC.md) for phase breakdown.

### `integrity.rs`

Non-interactive, 7-step pre-flight check. Runs before every `cordon run`. Returns `SystemConfig` on success; errors are fatal.

---

## `src/sandbox/`

Reads config, never writes it. All paths come from `system.toml` and `cordon.toml`.

### `builder.rs`

- `build_bwrap(project_path, network, dry_run)` — sets up namespace isolation flags, pseudo-filesystems, and the project directory bind.
- `apply_environment(bwrap, gui)` — adds `--setenv` args for safe env vars.

### `mounts.rs`

- `apply_system_mounts()` — iterates `system_config.mounts`, filters by `when`, skips unverified.
- `apply_user_mounts()` — reads `cordon.toml`; in normal mode, prompts before applying.

### `executor.rs`

**Integration point — calls everything else.** Key additions in Phase 2.8:

Before anything else, resolves any `--profile` argument. Then reads the project's `cordon.toml` and merges profile defaults into the effective flags. Resolution rules (lowest priority to highest): built-in defaults → named profile → cordon.toml → CLI flags.

```
effective_net     = cli_net if cli_net != Disable, else cordon.toml.network else profile.network
effective_gui     = cli_gui || cordon.toml.gui || profile.gui
effective_optional= deduplicate(cli_optional + cordon.toml.optional + profile.optional)
```

### `network.rs`

`NetworkMode` enum: `Disable` (default), `Allow` (proxy), `Full` (unrestricted).

### `proxy.rs`

Native Rust multi-threaded HTTP/HTTPS domain-filtering proxy. Started as a subprocess when `--net=allow`. Reads allowed domains from `proxy.toml` and `--domain` flags.

---

## Key Invariants

1. **Only `full_scan()` writes `system.toml`.** Nothing else does.
2. **`integrity_check()` never writes anything under normal operation.**
3. **No hardcoded paths in the sandbox module.** All paths come from `system.toml` and `cordon.toml`.
4. **`core.toml` is tamper-proof at runtime.** It lives in the binary.
5. **`verified = false` mounts are never passed to bwrap.**
6. **Exit codes are forwarded exactly.**

---

## Next

→ [PROGRESS.md](PROGRESS.md) — what's been built phase by phase, and what's coming next
