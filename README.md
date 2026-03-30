# Cordon

Cordon runs any command inside a restricted filesystem view on Linux. It uses Linux namespaces (via `bubblewrap`) to hide everything your process doesn't need — your home directory, SSH keys, AWS credentials, everything — while still letting the command do its job.

```bash
cordon run --net=allow -- npm install
cordon run -- bash setup.sh
cordon run --profile python -- pip install -r requirements.txt
```

No root. No containers. No daemons running in the background.

---

## Why?

When you run `npm install` or `pip install`, that code executes with your full user permissions. It can read `~/.ssh/id_rsa`, exfiltrate it to a server, modify files you own — all while appearing to just install a package.

This isn't paranoia. It's happened:

| Attack | What it did |
|--------|------------|
| **LiteLLM** (2024) | Malicious PyPI package read SSH keys and env vars on `pip install` |
| **xz-utils** (2024) | Build-time backdoor injected via the `make` step during install |
| **event-stream** (npm, 2018) | Compromised npm package silently stole Bitcoin wallets |
| **SolarWinds** | Supply chain attack through the build/update pipeline |

Cordon's answer: don't let the process see what it shouldn't, even if the code is malicious.

---

## How it works

Each `cordon run` creates a fresh isolated environment:

- `/usr`, `/bin`, `/lib` are mounted **read-only** — things can run, not break the system
- Your project directory is **writable** — `npm install` creates `node_modules/`, builds work
- `src/` is **read-only** if it exists — source files can't be silently rewritten
- `~/.ssh`, `~/.aws`, HOME — **not mounted at all** by default
- Network is **off** by default, or filtered through a built-in domain-allowlist proxy with `--net=allow`

When the process exits, the sandbox is gone. Nothing persists.

### The proxy

`--net=allow` mode spins up a built-in Rust HTTP/HTTPS proxy on a random localhost port and injects it into the sandbox via env vars (`HTTP_PROXY`, `https_proxy`, `npm_config_proxy`, etc.). It intercepts CONNECT tunnels and checks each domain against an allowlist before forwarding.

Default allowlist includes `registry.npmjs.org`, `pypi.org`, `crates.io`, `github.com`. Anything not on the list gets a 403 — even if the package's postinstall script tries to phone home.

```bash
# Only reaches npmjs.org and its CDNs — nothing else
cordon run --net=allow -- npm install

# Add a domain for a private registry
cordon run --net=allow --domain my.internal.registry -- npm install
```

### Seccomp

Optionally layer in a syscall filter:

```bash
# Blocks ptrace, kexec_load, mount, perf_event_open, process_vm_*
cordon run --seccomp basic --net=allow -- npm install

# Strict allowlist — only known-safe syscalls
cordon run --seccomp strict -- python3 untrusted.py
```

This is a second line of defence. Even if something breaks out of the filesystem restriction, it still can't call `ptrace` to attach to another process or `perf_event_open` for side-channel work.

---

## Install

```bash
git clone https://github.com/LORDv1shnu/Cordon
cd Cordon
bash install.sh        # builds release binary → ~/.local/bin/cordon
```

Or manually:
```bash
cargo build --release
cp target/release/cordon ~/.local/bin/
```

You need `bubblewrap` installed:
```bash
sudo apt install bubblewrap    # Ubuntu/Debian
sudo dnf install bubblewrap    # Fedora
sudo pacman -S bubblewrap      # Arch
```

First run after install:
```bash
cordon scan    # scans your system once, takes ~30s, writes ~/.config/cordon/system.toml
cordon check   # sanity check — makes sure everything's ready
```

---

## Usage

```bash
# The basics
cordon run -- echo "hello"
cordon run --net=allow -- npm install
cordon run --net=allow -- pip install -r requirements.txt
cordon run --net=full -- curl https://example.com   # unrestricted (no filtering)

# Seccomp
cordon run --seccomp basic --net=allow -- npm install

# GUI apps
cordon run --gui -- code .
cordon run --gui --optional audio_pipewire --optional dbus_session -- discord

# Built-in profiles (pre-configured for common runtimes)
cordon run --profile node -- npm install
cordon run --profile python -- python3 script.py
cordon run --profile rust -- cargo build

# Debug what's being blocked
cordon run --trace -- node server.js              # strace wrapper, reports denied paths
cordon add --from-trace ~/.config/cordon/logs/last-trace.log  # batch-add missing paths

# Resource limits (needs systemd)
cordon run --mem 512M --cpu 2.0 --timeout 60 -- npm install

# Dry run / verbose
cordon run --dry-run -- npm install               # print the bwrap command, don't run it
cordon run --verbose -- npm install               # print each bwrap arg as it runs
```

### Project setup (`cordon.toml`)

```bash
cordon init                            # auto-detects Cargo.toml / package.json / pyproject.toml
cordon set --net=allow                 # persist flags so you don't type them every time
cordon add /path/to/assets --mode ro   # expose extra paths into the sandbox
cordon edit                            # open cordon.toml in $EDITOR
```

### Inspect & debug

```bash
cordon check               # health check: bwrap, namespaces, AppArmor, modules
cordon doctor              # deeper: kernel version, distro quirks, exact fix suggestions
cordon status              # show what's in system.toml right now
cordon list                # show every mount that would be active on next run
cordon log --errors        # tail the last run's log, errors only
cordon syscalls --preset basic   # list what each seccomp preset blocks
```

### Profiles & portability

```bash
# Named profiles (stored in ~/.config/cordon/profiles.toml)
cordon profile create ci-node --net=allow --optional ld_so_cache
cordon run --profile ci-node -- npm test

# Lockfile for reproducible environments
cordon lock update          # SHA-256 all mount paths → cordon.lock
cordon lock verify          # check nothing drifted

# Share sandbox config with the team
cordon export > sandbox.json
cordon import sandbox.json
```

### Shell integration

```bash
# Tab completions
cordon completions zsh > ~/.zfunc/_cordon

# Transparent wrappers — make "npm" always run sandboxed
cordon wrap npm
cordon wrap pip
npm install                # actually runs: cordon run -- npm install "$@"
cordon unwrap npm

# Man page
cordon man | man -l -
```

---

## Configuration layers

There are four config files, applied in order:

| Layer | Where | Who writes it |
|-------|-------|---------------|
| `core.toml` | compiled into the binary | not editable at runtime (tamper-proof) |
| `system.toml` | `~/.config/cordon/` | `cordon scan` |
| `profiles.toml` | `~/.config/cordon/` | `cordon profile create` |
| `cordon.toml` | project directory | `cordon init` / `cordon set` / `cordon add` |

CLI flags always win. `cordon.toml` beats a named profile. The binary's built-in `core.toml` can't be touched at runtime.

---

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | success |
| 1 | cordon internal error |
| 2 | bad CLI usage |
| 125 | sandbox setup failed (bwrap missing, etc.) |
| 126 | command found but not executable inside the sandbox |
| 127 | command not found inside the sandbox |
| N | forwarded from the sandboxed process |

Same convention as `bwrap` and the shell.

---

## Limitations

Cordon is not a container, not an antivirus, and not a replacement for SELinux or AppArmor. It restricts filesystem visibility and network access — it's not trying to detect malware. If something has a kernel exploit, it'll break out. The goal is to make supply chain attacks significantly harder, not impossible.

Also: currently Linux x86_64 and aarch64 only. `bubblewrap` is required.

---

## Tech

Built in Rust. Uses `bubblewrap` for namespacing, `seccompiler` for BPF filter compilation, `clap` for CLI, `tracing` for structured logging. The domain-filtering proxy is written from scratch in pure Rust (no external proxy tool).

→ [COMMANDS.md](COMMANDS.md) — full flag reference  
→ [SCANNER_LOGIC.md](SCANNER_LOGIC.md) — how the system scanner works  
→ [MODULE_INFO.md](MODULE_INFO.md) — every source file explained  

---

*Built with AI assistance (Gemini). Architecture, security model, and design decisions are the author's work.*
