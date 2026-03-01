# Cordon

> Lightweight, per-execution filesystem sandbox for Linux.

Cordon lets you run any command inside a restricted filesystem view --> without modifying system-wide permissions, installing permanent policies, or using heavy virtualization.

```bash
cordon run -- npm install
cordon run --network -- curl https://example.com
```

---

## The Problem

When you run third-party code — AppImages, `pip install`, `npm install`, random `.sh` scripts — those programs run with **your full permissions**. They can read, modify, or delete anything you can.

Cordon reduces that risk by limiting what a program can even **see** during execution.

---

## How It Works

Cordon uses **Linux namespaces** (via `bubblewrap`) to create an isolated environment per execution:

- System directories (`/usr`, `/bin`, `/lib`) are mounted **read-only**
- Your project directory is mounted **writable**
- Sensitive subdirectories (like `src/`) are **overlaid as read-only**
- Everything else is **hidden**
- Network is **disabled by default**
- When the process exits, the sandbox is **gone entirely**

No root required. No system-wide config. No containers.

---

## What Cordon Is NOT

- Not an antivirus
- Not a malware scanner
- Not a container runtime
- Not a replacement for SELinux/AppArmor

It reduces risk through **filesystem restriction**, not detection.

---

## CLI Usage

```bash
# Run a command in sandbox (network off by default)
cordon run -- <command>

# Allow network access
cordon run --network -- <command>

# Dry-run: show what would be sandboxed without running
cordon run --dry-run -- <command>
```

---

## Phases

### Phase 1 — Core Sandbox (Week 1–2) `[IN PROGRESS]`

> Goal: A working sandbox runner with basic filesystem isolation.

| Feature | Status |
|---|---|
| Basic CLI (`cordon run -- <cmd>`) | ✅ Done |
| Spawn bubblewrap sandboxed process | ✅ Done |
| System dirs mounted read-only | ✅ Done |
| Project directory writable | ✅ Done |
| `src/` protected as read-only overlay | ✅ Done |
| Network disabled by default (`--network` flag) | 🔄 In Progress |
| Dry-run mode (`--dry-run` flag) | ⬜ Pending |
| Clean CLI output and UX polish | ⬜ Pending |
| Refactor and code cleanup | ⬜ Pending |

---

### Phase 2 — Observability (Week 3) `[PLANNED]`

> Goal: Show the user what the sandboxed program tried to do.

| Feature | Status |
|---|---|
| strace integration | ⬜ Planned |
| Blocked operation reporting | ⬜ Planned |
| Access attempt log output | ⬜ Planned |

---

### Phase 3 — Policy Files (Week 4) `[PLANNED]`

> Goal: Let users define sandbox rules via config file.

| Feature | Status |
|---|---|
| TOML policy file support | ⬜ Planned |
| Per-project sandbox profiles | ⬜ Planned |
| Built-in profiles (nodejs, python, rust) | ⬜ Planned |

---

### Phase 4 — CI & Distribution `[PLANNED]`

| Feature | Status |
|---|---|
| GitHub Actions smoke tests | ⬜ Planned |
| Demo script | ⬜ Planned |
| Prebuilt binary releases | ⬜ Planned |

---

## Building From Source

**Requirements:**
- Rust (install via [rustup](https://rustup.rs))
- `bubblewrap` installed (`sudo apt install bubblewrap` or equivalent)

```bash
git clone https://github.com/yourusername/cordon
cd cordon
cargo build
cargo run -- run -- echo "hello from sandbox"
```

---

## Target Users

- Developers running third-party install scripts
- Users testing AppImages or unknown binaries
- Contributors running scripts from open repositories
- Anyone who wants safer defaults without Heavy tooling

---

## Tech Stack

- **Rust** — systems programming language
- **bubblewrap** — Linux namespace sandboxing
- **clap** — CLI argument parsing
- **anyhow** — error handling
