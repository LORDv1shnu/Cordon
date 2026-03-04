# Cordon — Scanner Logic Design

> Internal design document. Work in progress.

---

## 🔴 Priority Tasks (Immediate Next Steps)

These three tasks must be completed in order before any other Phase 2 work.

### Task 1 — Finalise `core.toml` ✅ Done
Expand `core.toml` to cover all known runtime resources an app may need.
Split into categories: mandatory (app will not work without it) vs optional
(app works with reduced features). Add `required = true/false` field to each
module. Add developer comments explaining each module's purpose.
Also update `CoreModule` struct in `config.rs` to include the `required: bool` field.

### Task 2 — Complete the Scanner Module 🔴 In Progress
Finish `scanner.rs` according to the full design in this document:
- `integrity_check()` — file-first check against system.toml paths with fallback chain
- Foreign entry detection — check every system.toml name against core, prompt
  user [D]iscard / [M]ove to user.toml if foreign entry found
- Version mismatch detection — trigger main scan if binary version ≠ system.toml version
- Malformed system.toml handling — treat as empty, trigger main scan
- Hard fail on `--network` if any network module has `verified = false`
- Hard fail on `--gui` if any gui required module has `verified = false`
- Partial re-scan support — only re-check the affected module, never overwrite passing entries
- Resolve `$XDG_RUNTIME_DIR` env var at scan time for gui/audio module paths
- File lock on system.toml during write

### Task 3 — Link Scanner to sandbox.rs 🔴 Pending
Refactor `sandbox.rs` to:
- On first run (no system.toml in cwd): auto-trigger `scanner::run_scan()`
- On subsequent runs: call `scanner::pre_flight_check()` (quick scanner)
- Only proceed to spawn bwrap after scanner gives green flag
- Read all mount entries from system.toml instead of hardcoded paths
- Filter mounts by `when` field: always load `when = "always"`, load `when = "network"`
  only if `--network`, load `when = "gui"` only if `--gui`
- For each mount entry, apply correct bwrap arg based on `bind_type`:
  `ro-bind` → `--ro-bind src dest`, `symlink` → `--symlink src dest`,
  `rw-bind` → `--bind src dest`
- Read user.toml from `$HOME/.config/cordon/user.toml` and append those mounts
- Keep project dir bind and src/ overlay logic as-is (these are not in system.toml)

---

## The Three Files

| File | Purpose | Read by bwrap? | Written by |
|---|---|---|---|
| `core` (embedded in binary) | Blueprint — what to look for and why | ❌ Never | Us (compiled in) |
| `system.toml` | Scanner output — verified paths on this machine | ✅ Yes | Scanner |
| `user.toml` | User additions — extra dirs, project paths | ✅ Yes | User / TUI |

---

## Why core Is Embedded In The Binary

`core` is the master reference — it tells the scanner what paths to look for, what files must exist inside them, and what functionality each provides.

**If `core` can be tampered with on disk, the entire trust model collapses.**

A malicious actor could:
- Remove required security-critical paths from `core`
- Scanner would skip verifying them
- They'd never appear in `system.toml`
- bwrap would silently not mount critical isolation layers

**Solution: embed `core` directly into the binary using Rust's `include_str!()`.**

```rust
// Compiled into the binary at build time — cannot be changed without recompiling
const CORE: &str = include_str!("../config/core.toml");
```

This means:
- `core` ships as part of the binary
- Cannot be modified at runtime
- Updates to `core` require a new release
- Users can audit it in the source repo

---

## core Structure (What It Contains)

```toml
[[module]]
name           = "usr"
# Mandatory — entire /usr tree. Without this nothing runs.
description    = "Entire /usr tree — system binaries, libraries, shared data"
default_dir    = "/usr"
required_files = ["bin/sh", "lib"]
functionality  = "Without this, NO binary will execute inside the sandbox."
mode           = "ro"
when           = "always"
required       = true

[[module]]
name           = "dns_resolution"
# Required for --network. systemd-resolved stub, target of /etc/resolv.conf.
description    = "systemd-resolved stub directory — symlink target of /etc/resolv.conf."
default_dir    = "/run/systemd/resolve"
required_files = ["stub-resolv.conf"]
functionality  = "Without this, DNS resolution silently fails on systemd systems."
mode           = "ro"
when           = "network"
required       = true

[[module]]
name           = "x11_socket"
# Required for --gui. X11 display socket. App cannot show window without it.
description    = "X11 display socket directory — /tmp/.X11-unix."
default_dir    = "/tmp/.X11-unix"
required_files = []
functionality  = "Without this, X11 GUI apps cannot connect to the display."
mode           = "ro"
when           = "gui"
required       = true

[[module]]
name           = "dconf_runtime"
# Optional GUI. App still opens without it, but spams dconf warnings.
description    = "dconf/GSettings runtime directory — GNOME app preferences."
default_dir    = "/run/user/1000/dconf"
required_files = []
functionality  = "Without this, GNOME apps cannot save settings. App still runs."
mode           = "rw"
when           = "gui"
required       = false
```

**Key fields:**
- `when = "always"` → always loaded
- `when = "network"` → only loaded when `--network` is passed
- `when = "gui"` → only loaded when `--gui` is passed
- `when = "optional"` → not loaded by default, user opts in via user.toml
- `required = true` → sandbox will NOT start if this module is missing/unverified
- `required = false` → sandbox runs with reduced features, warnings shown

---

## system.toml Structure (Scanner Output)

```toml
# Generated by `cordon scan` — manual edits will be overwritten
# Last scan: 2026-03-01T21:00:00

# Regular bind mount (real directory on disk)
[[mount]]
name      = "base_libraries"
src       = "/usr/lib"
dest      = "/usr/lib"
bind_type = "ro-bind"     # real directory → use --ro-bind
mode      = "ro"
when      = "always"

# Symlink entry (src is the TARGET STRING, not a real path)
[[mount]]
name      = "bin"
src       = "usr/bin"     # string the symlink points to
dest      = "/bin"        # where to create symlink inside sandbox
bind_type = "symlink"     # symlink detected → use --symlink
when      = "always"

[[mount]]
name      = "dns_resolution"
src       = "/run/systemd/resolve"
dest      = "/run/systemd/resolve"
bind_type = "ro-bind"
mode      = "ro"
when      = "network"
```

**Key difference:**
- `bind_type = "ro-bind"` → `src` is a real path on disk, bwrap binds it
- `bind_type = "symlink"` → `src` is a target string, bwrap creates a symlink at `dest` pointing to `src`

---

## user.toml Structure (User Defined)

```toml
# User-defined mounts. Edit manually or via `cordon add` / TUI.

[[mount]]
src  = "/home/user/projects/myapp"
dest = "/project"
mode = "rw"
when = "always"

[[mount]]
src  = "/home/user/projects/myapp/src"
dest = "/project/src"
mode = "ro"
when = "always"
```

---

## Scanner Flow

```
cordon scan triggered
        │
        ▼
┌──────────────────────────────────┐
│  Is system.toml empty/not exist? │
└──────────────────────────────────┘
        │                       │
      YES                       NO
        │                       │
        ▼                       ▼
 ┌─────────────┐        ┌──────────────────┐
 │  FULL SCAN  │        │ INTEGRITY CHECK  │
 └─────────────┘        └──────────────────┘
        │                       │
        ▼                       ▼
 For each module          For each module
 in core:                 in core:
        │                       │
        ▼                       ▼
 Step 1:                  Look up module's
 Does default_dir         entry in system.toml
 exist on disk?           (by name)
        │                       │
  NO ───┤─── YES                ▼
  │         │             Do required_files
  ▼         ▼             exist at the path
 FAIL    Step 2:          listed in system.toml?
 report  Do required_           │
 missing files exist      YES ──┤── NO
 dir +   inside it?       │         │
 func    │                ▼         ▼
 loss    YES ─┐           OK    Does the core
  │      │   │                  default_dir appear
  │      NO  │                  in system.toml?
  │      │   │                       │
  │      ▼   ▼               YES ────┤──── NO
  │    FAIL PASS:                │         │
  │    report write to           ▼         ▼
  │    missing system.toml  Dir is there  Core path
  │    files + (also detect  but file is  not in
  │    func   symlink type   genuinely    system.toml
  │    loss   → ro-bind or   missing.     (corrected
  │           --symlink)     Ask user     path is stale)
  │                          for correct       │
  │                          location          ▼
  │                               │      Fall back to
  │                               │      core: does
  │                               │      core default_dir
  │                               │      exist on disk?
  │                               │           │
  │                               │     YES ──┤── NO
  │                               │     │         │
  │                               │     ▼         ▼
  │                               │  File there? FAIL:
  │                               │     │        inform
  │                               │  YES│NO      user,
  │                               │  add│FAIL:   ask for
  │                               │  to │report  path
  │                               │  sys│to user
  │                               │     │
  └─────────────────┬─────────────┘     │
                    ▼                   │
          Any failures across           │
          all modules?                  │
                    │                   │
              YES ──┤── NO              │
              │         │              │
              ▼         ▼              │
       Prompt user: "All modules    ───┘
       - what's      verified.
         missing     system.toml
       - what        is up to
         breaks      date."
       - ask for
         correct
         path
       - re-run
         scanner
         on that
         module
         only
         (no overwrite
         of passing
         entries)
```

---

## When Scanner Runs

| Trigger | Behavior |
|---|---|
| First ever `cordon run` | Auto-runs full scan (system.toml is empty) |
| `cordon scan` (manual) | Full scan, regenerates system.toml |
| Verification error at runtime | Auto-runs integrity check only |
| User provides missing path | Re-runs scanner on that module only |

---

## Security Properties

| Property | How |
|---|---|
| `core` cannot be tampered | Embedded in binary via `include_str!()` |
| `system.toml` is not exposed inside sandbox | bwrap only mounts what's listed, not the config files themselves |
| `user.toml` is not exposed inside sandbox | Same — its contents are mounted, not the file |
| Scanner never expands permissions silently | Failures are loud, not silent fallbacks |
| bwrap mounts ONLY what's in system + user | No implicit mounts, no "just in case" paths |

---

## Key Integrity Check Rules

1. **Integrity check does NOT compare paths** between core and system.toml — system.toml may have user-corrected paths that differ from core defaults. That's fine and expected.
2. **Integrity check checks files first**, not directories. Files are the ground truth.
3. **Fallback chain on file missing:**
   - File not found at system.toml path
   - → Check if core's `default_dir` is already in system.toml (same location, file just gone) → ask user
   - → If core path is NOT in system.toml (corrected path is stale) → try core's default on disk → if found, update system.toml → if not, ask user
4. **Partial re-run**: scanner only re-checks and re-writes the affected module. Passing modules are never touched.

---

## Symlink Detection (Cross-Distro Portability)

**At scan time**, for each directory being added to `system.toml`, the scanner checks via `fs::symlink_metadata()` whether the path is a symlink or a real directory.

- Real directory → write `bind_type = "ro-bind"` in system.toml
- Symlink → write `bind_type = "symlink"` + the symlink target in system.toml

bwrap then uses whatever `bind_type` is specified in system.toml. No hardcoded assumptions. Works on merged-usr (Ubuntu/Debian) and non-merged (Arch, older systems) alike.

---

## Foreign Entry Detection

`system.toml` is for **barebone system modules only** — strictly what core defines. Nothing else.

If an entry in system.toml has a `name` that does not match any module in core, it is considered **foreign**.

A foreign entry in system.toml is a security risk: it could be injected to force bwrap to expose an unintended path at startup.

### What Happens On Detection

```
Scanner reads system.toml
        │
        ▼
For each entry in system.toml:
   Is entry name in core?
        │
   YES ─┤─── NO
   │             │
   OK            ▼
           FOREIGN ENTRY detected
                 │
                 ▼
           Block bwrap from starting
                 │
                 ▼
           Inform user:
           - entry name
           - src/dest path
           - "this does not belong to core modules"
                 │
                 ▼
           Ask user:
           [D] Discard — remove from system.toml
           [M] Move    — move to user.toml instead
                 │
           ┌─────┴──────┐
           ▼            ▼
       remove        append to
       from          user.toml,
       system.toml   remove from
                     system.toml
                 │
                 ▼
           Re-run scanner
           (now system.toml is clean)
                 │
                 ▼
           bwrap allowed to start
```

### Where This Check Runs

| Trigger | Behavior |
|---|---|
| Before every `cordon run` | Quick check: any foreign entries in system.toml? |
| During `cordon scan` | Full check as part of integrity pass |

This is a **pre-flight check**, not just a scan-time check. bwrap never starts if system.toml is dirty.

---

## Two Scanner Programs

### Main Scanner
- Runs on: first ever `cordon run`, manual `cordon scan`, when a problem is detected
- Does: full two-step verification (dir exists → files exist inside it)
- Writes to: system.toml
- Handles: user prompts for missing paths, partial re-runs, foreign entry cleanup

### Quick Scanner
- Runs on: before every `cordon run` (pre-flight)
- Does: lightweight file-only integrity check against current system.toml paths
- Does NOT write to system.toml
- Triggers main scanner if anything fails

---

## Pre-flight Execution Order (before every `cordon run`)

```
1. Parse system.toml
      └── Malformed? → treat as empty → trigger Main Scanner

2. Version check
      └── Binary version ≠ system.toml version? → trigger Main Scanner

3. Foreign entry check (Quick Scanner)
      └── Entry in system.toml not in core? → block bwrap → prompt user

4. Network module check (only if --network passed)
      └── Any network module has verified = false? → hard fail, inform user

5. All clear → spawn bwrap
```

---

## Exact Match Rule (Finalized)

system.toml must contain **exactly** the files defined in core — no more, no less.
- **Locations can differ** (user-corrected paths are fine)
- **Files must match exactly** — if a file in core is missing from system.toml, it's a failure
- **Extra entries** (foreign) → blocked, prompt Discard or Move-to-user.toml

---

## verified = false Behavior

When a `when = "network"` module fails verification during full scan:
- Entry is still written to system.toml
- Marked with `verified = false`
- Quick scanner shows it as a warning, not a failure
- At runtime: if user passes `--network` and any network module has `verified = false` → **hard fail**, inform user which module failed and why

---

## Decisions Made

| Decision | Choice |
|---|---|
| `system.toml` location | Per-project: `./system.toml` inside the current working directory (project-local, generated by first run or `cordon scan`) |
| `user.toml` location | User-global: `~/.config/cordon/user.toml` (applies to all projects for this user) |
| `core` tamper protection | Embedded in binary via `include_str!()` |
| Symlink vs ro-bind | Runtime detection by scanner, stored as `bind_type` in system.toml |
| Symlink entries in system.toml | `src` = target string, `dest` = link path, `bind_type = "symlink"` |
| User-corrected paths | Stored in system.toml under same module name, replaces default |
| `cordon add` behavior | Writes to `user.toml`; TUI calls the same add command internally |
| Foreign entries in system.toml | Block bwrap, inform user, offer Discard or Move-to-user.toml |
| user.toml auto-create | If Move chosen and user.toml doesn't exist, create it automatically |
| Foreign check scope | Quick scanner checks system.toml only. user.toml has its own separate security program (future) |
| Two scanner types | Main Scanner (full, writes), Quick Scanner (pre-flight, read-only) |
| Exact match enforcement | Files must match core exactly. Locations can differ (user corrections OK). |
| `verified = false` entries | Written to system.toml, warning only until `--network` is used, then hard fail |
| Version mismatch | Trigger main scan automatically |
| Malformed system.toml | Treat as empty, trigger main scan |
| Concurrent scan processes | File lock on system.toml during write |
| Finding `cordon.toml` | Walk UP from cwd toward /home until found. None = run without user mounts. |
| `core.toml` module list | Finalized — covers always/network/gui/optional categories with required flag |
| `required` field in modules | true = hard fail if missing, false = warning + degraded mode |
| `when` values | "always", "network", "gui", "optional" |
| `$XDG_RUNTIME_DIR` paths | Resolved at scan time via env var, stored as actual path in system.toml |
| GUI module failure behaviour | required=true missing → hard fail; required=false missing → warn in stderr, continue |
| Exit code strategy | To be discussed |

---

## Tasks

### Scanner — Phase 2 Implementation

- [x] Define `CoreModule` struct (name, description, default_dir, required_files, functionality, mode, when)
- [ ] Add `required: bool` field to `CoreModule` struct in `config.rs` and update TOML deserialization
- [x] Define `CoreConfig`, `SystemConfig`, `MountEntry`, `UserConfig`, `UserMount` structs in `config.rs`
- [x] Embed `core.toml` in binary using `include_str!()`
- [x] Write `core.toml` with all required modules (usr, bin, lib, lib64, sbin, dns_resolution, ssl_certificates)
- [x] Expand `core.toml` with gui modules (x11_socket, fonts, wayland_runtime, dconf_runtime, dbus_session, gpu_dri)
- [x] Expand `core.toml` with optional modules (locale_files, timezone, ld_so_cache, audio_pipewire, audio_pulse)
- [x] Add `required = true/false` field and developer comments to every module in `core.toml`
- [x] Implement `parse_core()` — deserialize embedded core into `Vec<CoreModule>` via `toml::from_str()`
- [x] Implement `full_scan()` — two-step verify for each module, write to system.toml
- [x] Implement symlink detection — `fs::symlink_metadata()` → choose `bind_type` (ro-bind vs symlink)
- [x] Mark network modules as `verified = false` when files missing (warn, not fail)
- [x] Store `cordon_version` in system.toml header
- [x] Implement `find_user_config()` in `config.rs` — walks up from cwd toward / for cordon.toml
- [x] Implement `save_system_config()` — writes system.toml to `~/.config/cordon/`
- [x] Wire `cordon scan` subcommand to scanner
- [ ] Implement `integrity_check()` — file-first check against system.toml paths, fallback chain
- [ ] Implement foreign entry detection — check every system.toml entry name against core
- [ ] Implement user prompt for foreign entries — [D]iscard / [M]ove to user.toml
- [ ] Auto-create user.toml if it doesn't exist on Move
- [ ] Version mismatch detection — binary version ≠ system.toml version → trigger main scan
- [ ] Handle malformed system.toml — parse error → treat as empty → rescan
- [ ] Hard fail on `--network` if any network module has `verified = false`
- [ ] Implement file lock on system.toml during write (prevent concurrent scan corruption)
- [ ] Main Scanner: prompt user for corrected paths on missing modules
- [ ] Main Scanner: partial re-run — only re-check affected module, never overwrite passing entries
- [ ] Auto-trigger full scan on first `cordon run` (system.toml missing/empty)
- [ ] Auto-trigger integrity check on verification error at runtime
- [ ] Replace hardcoded bwrap paths in `sandbox.rs` with entries read from system.toml + user.toml
- [ ] Wire `find_user_config()` into sandbox execution path (user.toml global mounts)
- [ ] Update system.toml location: per-project `./system.toml` in cwd
- [ ] Update user.toml location: global `~/.config/cordon/user.toml`
- [ ] Resolve `$XDG_RUNTIME_DIR` at scan time for gui/audio module paths
- [ ] Filter mounts by `when` field when building bwrap command (always/network/gui/optional)
- [ ] Apply correct bwrap arg per `bind_type`: ro-bind / symlink / bind (rw)
- [ ] Hard fail if any `required = true` gui module has `verified = false` and `--gui` is passed
