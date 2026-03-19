# Cordon — Scanner & Architecture Notes

> **Reading order:** [README.md](README.md) → [COMMANDS.md](COMMANDS.md) → **SCANNER_LOGIC.md** → [MODULE_INFO.md](MODULE_INFO.md) → [PROGRESS.md](PROGRESS.md)
>
> This document covers internal scanner design. For per-file developer docs, see [MODULE_INFO.md](MODULE_INFO.md).

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│  core.toml   — compiled into binary at build time (read-only)    │
│  system.toml — scanner output in ~/.config/cordon/ (per machine) │
│  cordon.toml — optional per-project user mounts (per project)    │
└──────────────────────────────────────────────────────────────────┘

cordon run   →  integrity_check()  →  build_bwrap()  →  exec
cordon scan  →  full_scan()        →  write system.toml
cordon add   →  add_user_mount()   →  write/update cordon.toml
cordon set   →  set_profile_field()→  write/update cordon.toml
```

---

## Two-Mode Scanner Design

### `full_scan()` — Interactive, writes system.toml

The only function that writes to disk. Run on first use or via `cordon scan`.

- **Phase 1** — Mandatory (`when = "always"`): scanned automatically, no choice.
- **Phase 2** — Network (`when = "network"`): single yes/no covers the whole group.
- **Phase 3** — GUI (`when = "gui"`): single yes/no covers the whole group.
- **Phase 4** — Optional (`when = "optional"`): each module described individually before asking.

Required modules not found → user prompted for corrected path (handles NixOS / non-FHS layouts).

### `integrity_check(network, gui)` — Non-interactive, runs before every `cordon run`

Never writes to disk under normal conditions.

| Step | Check | On failure |
|------|-------|------------|
| 1 | Parse `system.toml` | Trigger `full_scan` |
| 2 | Version check (binary ≠ `cordon_version` in file) | Trigger `full_scan` |
| 3 | Foreign entry (unknown module name) | Hard block — security gate |
| 4 | Verified paths still exist on disk | Trigger `full_scan` |
| 5 | Required `always` modules are verified | Hard block |
| 6 | `--network` gate: required network modules verified | Hard block |
| 7 | `--gui` gate: required GUI modules verified | Hard block |

---

## Design Decisions

**Why is `core.toml` compiled into the binary?**
Prevents runtime tampering. A sandboxed process cannot change what modules Cordon knows about.

**Why two config files (`system.toml` / `cordon.toml`)?**
System paths are machine-specific. Project paths are project-specific. Mixing them would mean
a committed `cordon.toml` encodes absolute paths from the developer's machine.

**Why `fd-lock` on `system.toml` writes?**
`cordon scan` can be Ctrl-C'd mid-write. `fd-lock` ensures no partial write is visible to a concurrent `cordon run`.

**Why re-trigger `full_scan` on version mismatch?**
A version bump may add/remove/rename modules. Migration logic would be fragile. A fresh scan is always correct.

**Why does `add_user_mount` set `src == dest`?**
Mounting at an aliased path creates surprising results inside the sandbox. Same src/dest is
predictable and auditable.

**Why is `DBUS_SESSION_BUS_ADDRESS` not forwarded as an env var?**
The socket is bind-mounted directly via `system.toml`. Forwarding the address would let the
sandboxed process reach outside the mount namespace. Binding the socket file is safer.

---

## Next

→ [MODULE_INFO.md](MODULE_INFO.md) — per-file breakdown of every source file in `src/`
