# Cordon

> Lightweight, per-execution filesystem sandbox for Linux — stop supply chain attacks before they start.

```bash
cordon run --net=allow -- npm install      # safe: npm can only reach npmjs.org
cordon run --net=allow -- pip install -r requirements.txt  # safe: only pypi.org
cordon run -- bash suspicious.sh           # no network, no home dir, no secrets
```

---

## The Problem: Supply Chain Attacks

When you run third-party code — `npm install`, `pip install`, random `.sh` scripts, AppImages — those programs run with **your full permissions**. They can:

- Read `~/.ssh/id_rsa`, `~/.aws/credentials`, `~/.gnupg/`
- Exfiltrate secrets to an attacker's server
- Modify files anywhere you have write access
- Phone home in the background while appearing to install packages

This is not hypothetical. Recent real-world attacks:

| Attack | What happened |
|--------|--------------|
| **LiteLLM** (2024) | Malicious PyPI package `litellm` exfiltrated SSH keys and env vars on `pip install` |
| **xz-utils** (2024) | Backdoor injected through the build pipeline — `make` during install was the attack surface |
| **event-stream** (npm, 2018) | Dependency injected into npm package to steal Bitcoin wallets |
| **SolarWinds** | Supply chain compromise via a build-time injected update package |

Cordon reduces that risk by limiting what a program can even **see** during execution.

---

## How It Works

Cordon uses **Linux namespaces** via `bubblewrap` to create an isolated environment per execution:

- System directories (`/usr`, `/bin`, `/lib`) are mounted **read-only** — the command can run but not write to system paths
- Your **project directory is writable** — `npm install` still works, `node_modules/` gets created
- `src/` (if it exists) is protected as **read-only** — source code can't be silently modified
- `~/.ssh`, `~/.aws`, `~/.gnupg` and your entire home directory are **hidden** — nothing to steal
- Network is **disabled by default** — or filtered through a domain-allow-list proxy in `--net=allow` mode
- When the process exits, the sandbox is **gone entirely** — no persistent state

No root. No containers. No system-wide config.

### Defence Against `npm install` Style Attacks

```bash
# The "node" built-in profile allows only registry.npmjs.org + ld.so.cache
cordon run --profile node -- npm install

# Or manually:
cordon run --net=allow -- npm install
# Proxy allows: registry.npmjs.org, npmjs.org, nodejs.org, github.com
# Everything else → 403 Forbidden  (even if the package tries to exfiltrate)
```

The domain-filtering **proxy is built into Cordon** — no external tool needed. It intercepts HTTPS via CONNECT tunneling and checks each target against the allow-list before connecting.

### Seccomp Adds a Kernel-Level Safety Net

```bash
# Block dangerous syscalls (ptrace, kexec, mount, perf_event_open…)
cordon run --seccomp basic --net=allow -- npm install

# Strict allow-list: only known-safe syscalls pass through
cordon run --seccomp strict -- python3 script.py
```

Even if someone escapes the filesystem restriction, seccomp blocks the syscalls needed to pivot further (e.g. `ptrace` to attach to other processes, `perf_event_open` for side-channel attacks).

---

## Four-Layer Config

| Layer | File | Description |
|-------|------|-------------|
| Core | `core.toml` (in binary) | Blueprint of what paths to look for. Immutable at runtime. |
| System | `~/.config/cordon/system.toml` | Scanner output — verified paths on **this machine**. |
| Profile | `~/.config/cordon/profiles.toml` | Global reusable sandbox configuration profiles. |
| Project | `./cordon.toml` | Optional per-project extra mounts and profile defaults. |

bwrap reads paths only from `system.toml` and `cordon.toml`. Neither file is ever exposed inside the sandbox. `core.toml` is compiled into the binary — tamper-proof at runtime.

---

## Quick Start

```bash
# Option 1: Install script (builds release binary)
git clone https://github.com/LORDv1shnu/Cordon
cd Cordon
bash install.sh

# Option 2: Build manually
cargo build --release
cp target/release/cordon ~/.local/bin/cordon
```

**Requirements:** Rust (via [rustup](https://rustup.rs)) + `bubblewrap`:

```bash
# Ubuntu/Debian
sudo apt install bubblewrap

# Fedora/RHEL
sudo dnf install bubblewrap

# Arch
sudo pacman -S bubblewrap
```

```bash
# First run: interactive system scan (~30 seconds)
cordon scan

# Run any command sandboxed
cordon run -- echo "hello from sandbox"

# Safe npm install (blocks exfiltration)
cordon run --net=allow -- npm install

# Safe pip install
cordon run --net=allow -- pip install -r requirements.txt
```

---

## CLI Reference

```bash
# Run a command (network disabled by default — most restrictive)
cordon run -- <command>

# Domain-filtered network access (proxy — recommended for package managers)
cordon run --net=allow -- npm install
cordon run --net=allow --domain custom.registry.com -- npm install

# Full unrestricted network access (use only when needed)
cordon run --net=full -- curl https://example.com

# Apply seccomp syscall filter for extra kernel-level protection
cordon run --seccomp basic --net=allow -- npm install

# Enable GUI app support (X11/Wayland/fonts)
cordon run --gui -- code .

# Activate optional modules (e.g. audio, dbus)
cordon run --optional audio_pipewire --optional dbus_session -- discord

# Dry-run: show the bwrap command without executing it
cordon run --dry-run -- npm install

# Debug: verbose tracing output on stderr + log file
cordon run --debug -- node server.js

# Trace: check what your app was trying to access but was denied
cordon run --trace -- node server.js
cordon add --from-trace ~/.config/cordon/logs/last-trace.log

# Resource limits (requires systemd)
cordon run --mem 512M --cpu 2.0 --timeout 60 -- npm install

# Use built-in profiles (node, python, rust, gui-app)
cordon run --profile node -- npm install
cordon run --profile python -- pip install -r requirements.txt

# Health-check: verify bwrap, namespaces, AppArmor, and module readiness
cordon check

# Deep diagnostic report with suggested fixes
cordon doctor

# Re-scan the system (after upgrades, new distro, etc.)
cordon scan

# Show all mounts that would be active in the next sandbox run
cordon list

# View the log generated by the last sandbox run
cordon log
cordon log --last 5
cordon log --errors

# Manage per-project cordon.toml
cordon init                   # scaffold from project type auto-detection
cordon add /path/to/dir --mode rw
cordon remove /path/to/dir
cordon edit
cordon set --net=allow --gui --optional audio_pipewire
cordon unset --net --gui

# Named sandbox profiles
cordon profile create myprofile --net=allow --optional ld_so_cache
cordon profile list
cordon profile show myprofile
cordon run --profile myprofile -- node server.js

# Show system.toml contents without scanning
cordon status

# Reproducible sandbox specs (lockfile)
cordon lock update
cordon lock verify

# Export/Import portable sandbox specifications (JSON)
cordon export > spec.json
cordon import spec.json

# List syscalls blocked or allowed by each preset
cordon syscalls --preset basic

# Generate shell completions (bash, zsh, fish, powershell, elvish)
cordon completions zsh > ~/.zfunc/_cordon

# Create/remove transparent shell wrappers for sandboxed commands
cordon wrap npm      # now "npm" always runs sandboxed
cordon wrap pip
cordon unwrap npm

# Generate the cordon.1 man page
cordon man
cordon man > ~/.local/share/man/man1/cordon.1
```

> Full flags, examples, and details: **[COMMANDS.md](COMMANDS.md)**

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
- Not a container runtime (no images, no daemons, no registry)
- Not a replacement for SELinux / AppArmor
- Not a VM

It reduces risk through **filesystem restriction + network allow-listing + syscall filtering**, not detection. Think of it as a mandatory access control wrapper for ad-hoc command execution.

---

## Tech Stack

| Crate / Tool | Role |
|---|---|
| `bubblewrap` | Linux namespace sandboxing (filesystem isolation) |
| `clap` | CLI argument parsing |
| `anyhow` + `thiserror` | Error handling — typed `CordonError` with per-variant messages |
| `tracing` + `tracing-subscriber` + `tracing-appender` | Structured dual-sink logging (stderr + file) |
| `seccompiler` | Pure-Rust BPF seccomp filter compilation |
| `serde` + `toml` + `serde_json` | Config serialisation |
| `sha2` | SHA-256 for lockfile integrity |
| `fd-lock` | Write lock on `system.toml` during scans |
| `chrono` | Timestamps in `system.toml` |
| `clap_complete` + `clap_mangen` | Shell completions + man page generation |

---

## Source Layout

```
Cordon/
├── Cargo.toml              # Rust manifest + dependencies
├── install.sh              # One-liner install script
├── .github/workflows/ci.yml  # GitHub Actions CI
├── config/
│   └── core.toml           # Module blueprint (compiled into binary)
├── src/
│   ├── main.rs             # Entry point — routes CLI to modules, nothing else
│   ├── cli.rs              # Argument structs (clap). No logic.
│   ├── config.rs           # Data types + file I/O for all three config layers
│   ├── errors.rs           # CordonError typed enum (thiserror)
│   ├── logger.rs           # Dual-sink tracing logger (stderr + log file)
│   ├── suggestions.rs      # Smart "did you mean?" suggestions & synopses
│   ├── distro.rs           # Distro detection (NixOS, Standard)
│   ├── wrapper.rs          # Shell wrapper script management (~/.local/bin)
│   ├── commands/           # Standalone subcommand implementations
│   │   ├── check.rs        # cordon check
│   │   ├── list.rs         # cordon list
│   │   ├── profile.rs      # cordon profile
│   │   ├── status.rs       # cordon status
│   │   ├── syscalls.rs     # cordon syscalls
│   │   ├── log.rs          # cordon log
│   │   ├── init.rs         # cordon init
│   │   ├── doctor.rs       # cordon doctor
│   │   ├── lock.rs         # cordon lock
│   │   └── spec.rs         # cordon export/import
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
│       ├── proxy.rs        # Native Rust domain-filtering HTTP/HTTPS proxy
│       ├── seccomp.rs      # Seccomp BPF filter generation
│       └── tracer.rs       # Wrap bwrap with strace to detect denied paths
├── COMMANDS.md             # Full command reference
├── MODULE_INFO.md          # Developer guide and repository breakdown
├── PROGRESS.md             # What's been built, what's planned
├── README.md               # This file
├── SCANNER_LOGIC.md        # Internal scanner design and architecture
```

---

## Further Reading

| Document | What you'll find |
|----------|--------------------|
| [COMMANDS.md](COMMANDS.md) | Every flag, all subcommands, network profiles, optional modules |
| [SCANNER_LOGIC.md](SCANNER_LOGIC.md) | How the scanner and integrity check work internally |
| [MODULE_INFO.md](MODULE_INFO.md) | Developer guide — every source file explained |
| [PROGRESS.md](PROGRESS.md) | What's been built, what's planned |

---

## AI Usage Note

Built with AI assistance (Gemini) as an implementation accelerator.
All architecture decisions, security model, and design direction are the author's work.
