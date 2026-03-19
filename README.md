# Cordon

> Lightweight, per-execution filesystem sandbox for Linux.

Run any command inside a restricted filesystem view — without modifying system-wide permissions, installing permanent policies, or using heavy virtualisation.

```bash
cordon run -- ls -la
cordon run --net=full -- curl https://example.com
cordon run --net=allow --domain google.com -- curl https://google.com
cordon run --gui -- code .
```

---

## The Problem

When you run third-party code — `npm install`, `pip install`, random `.sh` scripts, AppImages — those programs run with **your full permissions**. They can read, modify, or delete anything you can.

Cordon reduces that risk by limiting what a program can even **see** during execution.

---

## How It Works

Cordon uses **Linux namespaces** via `bubblewrap` to create an isolated environment per execution:

- System directories (`/usr`, `/bin`, `/lib`) are mounted **read-only**
- Your project directory is mounted **writable**
- `src/` (if it exists) is protected as **read-only**
- Everything else is **hidden**
- Network is **disabled by default** (isolated namespace)
- **Domain filtering proxy**: Only allowed domains can be reached in `--net=allow` mode
- When the process exits, the sandbox is **gone entirely**

No root. No containers. No system-wide config.

### Three-Layer Config

| Layer | File | Description |
|-------|------|-------------|
| Core | `core.toml` (in binary) | Blueprint of what paths to look for. Immutable at runtime. |
| System | `~/.config/cordon/system.toml` | Scanner output — verified paths on **this machine**. |
| Project | `./cordon.toml` | Optional per-project extra mounts. |

bwrap reads paths only from `system.toml` and `cordon.toml`. Neither file is ever exposed inside the sandbox.

---

## Quick Start

```bash
git clone https://github.com/yourusername/cordon
cd cordon
cargo build --release

# First run triggers an interactive system scan (~30 seconds)
cargo run -- run -- echo "hello from sandbox"
```

**Requirements:** Rust (via [rustup](https://rustup.rs)) + `bubblewrap` (`sudo apt install bubblewrap`).

---

## CLI Reference

```bash
# Run a command (network disabled by default)
cordon run -- <command>

# Network access (disabled by default)
cordon run --net=disable -- <command>

# Domain-filtered network access (proxy)
cordon run --net=allow -- <command>
cordon run --net=allow --domain google.com -- <command>

# Full unrestricted network access
cordon run --net=full -- <command>

# Enable GUI app support (X11/Wayland/fonts)
cordon run --gui -- <command>

# Activate optional modules (e.g. audio, dbus)
cordon run --optional audio --optional dbus -- <command>

# Dry-run: show the bwrap command without executing it
cordon run --dry-run -- <command>

# Debug: verbose tracing output on stderr + log file
cordon run --debug -- <command>

# Re-scan the system (after upgrades, new distro, etc.)
cordon scan

# Health-check: verify bwrap, namespaces, AppArmor, and module readiness
cordon check

# Show all mounts that would be active in the next sandbox run
cordon list

# Add a custom path to the per-project cordon.toml
cordon add /path/to/dir --mode rw

# Set default profile flags in the per-project cordon.toml
cordon set --net=allow --gui --optional audio_pipewire

# Unset default profile flags from the per-project cordon.toml
cordon unset --net --gui --optional audio_pipewire
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Cordon internal error (config, scan, etc.) |
| 2 | Bad CLI usage |
| 125 | Sandbox setup failed (bwrap missing, scan error) |
| 126 | Command found but not executable inside sandbox |
| 127 | Command not found inside sandbox |
| N | Any other code — forwarded from the sandboxed process |

Codes 125–127 follow the same convention as `bwrap` and the shell.

---

## What Cordon Is NOT

- Not an antivirus or malware scanner
- Not a container runtime
- Not a replacement for SELinux / AppArmor

It reduces risk through **filesystem restriction**, not detection.

---

> For roadmap and progress tracking, see [PROGRESS.md](PROGRESS.md).

---

## Target Users

- Developers running third-party install scripts (`npm install`, `pip install`)
- Users testing AppImages or unknown binaries
- Contributors running scripts from open-source repositories
- Anyone who wants safer defaults without heavy tooling

---

## Tech Stack

| Crate / Tool | Role |
|---|---|
| `bubblewrap` | Linux namespace sandboxing |
| `clap` | CLI argument parsing |
| `anyhow` | Error handling and propagation |
| `thiserror` | Typed `CordonError` enum with per-variant messages |
| `tracing` + `tracing-subscriber` | Structured logging (stderr + file) |
| `tracing-appender` | Non-blocking file sink for `last-run.log` |
| `serde` + `toml` | Config serialisation |
| `chrono` | Timestamps in `system.toml` |
| `fd-lock` | Write lock on `system.toml` during scans |

---

## Source Layout

```
src/
 ├── main.rs          # CLI router, exit code handling, diagnostic error box
 ├── cli.rs           # clap argument structs (no logic)
 ├── config.rs        # Data types + file I/O for all three config layers
 ├── errors.rs        # CordonError typed enum (thiserror)
 ├── logger.rs        # Dual-sink tracing logger (stderr + ~/.config/cordon/logs/)
 ├── commands/
 │   ├── mod.rs
 │   ├── check.rs         # cordon check — sandbox health check
 │   └── list.rs          # cordon list — show active mounts
 ├── scanner/
 │   ├── mod.rs
 │   ├── full_scan.rs     # Interactive 4-phase scanner, writes system.toml
 │   ├── integrity.rs     # Non-interactive 7-step pre-flight check
 │   ├── module_scan.rs   # Per-module scan logic (symlink detection, D-Bus)
 │   └── env_resolver.rs  # XDG_RUNTIME_DIR + D-Bus socket path resolution
 └── sandbox/
     ├── mod.rs
     ├── builder.rs       # Builds base bwrap Command + env var passthrough
     ├── mounts.rs        # Applies system + user mounts to bwrap command
     ├── network.rs       # NetworkMode enum
     ├── proxy.rs         # Native Rust domain-filtering HTTP/HTTPS proxy
     └── executor.rs      # Orchestrates the full cordon run flow
```

See [MODULE_INFO.md](MODULE_INFO.md) for a detailed description of every file.

---

## AI Usage Note

Built with AI assistance (GitHub Copilot / Claude) as an implementation accelerator.
All architecture decisions, security model, and design direction are the author's work.
See [SCANNER_LOGIC.md](SCANNER_LOGIC.md) for design rationale.
