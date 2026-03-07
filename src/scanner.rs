//! scanner.rs
//!
//! Two-mode scanner architecture:
//!
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │  FULL SCAN  (`full_scan`)                                           │
//! │                                                                     │
//! │  Interactive. Run on first use or via `cordon scan`.                │
//! │  Phase 1 — Mandatory (always) modules scanned automatically.        │
//! │  Phase 2 — Asks: "Include network support?"                         │
//! │  Phase 3 — Asks: "Include GUI support?"                             │
//! │  Phase 4 — Lists optional modules one by one, user opts in/out.     │
//! │  Writes result to system.toml.                                      │
//! └─────────────────────────────────────────────────────────────────────┘
//!
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │  INTEGRITY CHECK  (`integrity_check`)                               │
//! │                                                                     │
//! │  Non-interactive. Runs before every `cordon run`.                   │
//! │  Only checks what is already in system.toml — nothing more.         │
//! │  Step 1 — Parse system.toml (malformed → trigger full scan).        │
//! │  Step 2 — Version check (mismatch → trigger full scan).             │
//! │  Step 3 — Foreign entry check (block if found).                     │
//! │  Step 4 — File existence check for each verified mount.             │
//! │  Step 5 — Hard fail if --network modules are missing/unverified.    │
//! │  Step 6 — Hard fail if --gui modules are missing/unverified.        │
//! │  Returns SystemConfig on success, error on failure.                 │
//! └─────────────────────────────────────────────────────────────────────┘

use anyhow::{bail, Context, Result};
use crate::config::{CoreConfig, CoreModule, MountEntry, SystemConfig};
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use chrono::Utc;

// Embedded blueprint — compiled into the binary at build time.
// Cannot be tampered with at runtime. Any change requires a rebuild.
const CORE_TOML: &str = include_str!("../config/core.toml");

// ─────────────────────────────────────────────────────────────────
// HELPERS
// ─────────────────────────────────────────────────────────────────

/// Resolves `/run/user/1000` placeholder to the real `$XDG_RUNTIME_DIR`.
///
/// Called ONCE at scan time. The resolved concrete path is stored in
/// system.toml so the quick check and bwrap never need to touch env vars.
fn resolve_env_vars(path: &str) -> String {
    if path.contains("/run/user/1000") {
        if let Ok(val) = std::env::var("XDG_RUNTIME_DIR") {
            return path.replace("/run/user/1000", &val);
        }
    }
    path.to_string()
}

/// Print a yes/no prompt and return true only if the user types 'y' or 'Y'.
/// Any other input (including pressing Enter directly) defaults to No.
fn ask_yes_no(prompt: &str) -> bool {
    print!("{} [y/N]: ", prompt);
    io::stdout().flush().unwrap_or(());
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap_or(0);
    input.trim().eq_ignore_ascii_case("y")
}

/// When a required module is not found at its default path, ask the user
/// for a corrected path. Returns None if the user presses Enter (skips it).
fn ask_for_path(module_name: &str, tried_path: &str) -> Option<String> {
    println!();
    println!("     Not found at: {}", tried_path);
    println!("     Enter a corrected path for '{}', or press Enter to skip:", module_name);
    print!("     > ");
    io::stdout().flush().unwrap_or(());
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap_or(0);
    let trimmed = input.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

// ─────────────────────────────────────────────────────────────────
// FULL SCAN — interactive, writes system.toml
// ─────────────────────────────────────────────────────────────────

/// Full system scan — interactive.
///
/// This is the only function that WRITES to system.toml.
/// Run it once on first use, or again via `cordon scan` when needed.
///
/// Design principle: system.toml should only contain what THIS user,
/// on THIS machine, for THEIR use case actually needs.
/// We never silently add things the user didn't ask for.
///
/// Phase 1: Mandatory (when = "always") — scanned automatically.
///   No choice here. Every sandbox needs /usr, /bin, /lib, etc.
///
/// Phase 2: Network (when = "network") — single yes/no per feature group.
///   Only add if user plans to use --network. Fewer mounts = less exposure.
///
/// Phase 3: GUI (when = "gui") — single yes/no per feature group.
///   Only add if user plans to run GUI apps. Same reasoning.
///
/// Phase 4: Optional (when = "optional") — asked per-module with explanation.
///   User sees what each module does before deciding.
pub fn full_scan() -> Result<()> {
    println!();
    println!("╔══════════════════════════════════════════╗");
    println!("║       Cordon — Full System Scan          ║");
    println!("╚══════════════════════════════════════════╝");
    println!();
    println!("Verifies which system paths exist on this machine");
    println!("and writes system.toml that the sandbox reads at runtime.");
    println!();

    let core: CoreConfig = toml::from_str(CORE_TOML)
        .context("Failed to parse embedded core.toml")?;

    let mut mounts: Vec<MountEntry> = Vec::new();

    // ── Phase 1: Always-mandatory modules (no prompting)
    println!("[ Phase 1: Mandatory system modules ]");
    println!("  Required for any binary to execute. Not skippable.");
    println!();

    for module in core.modules.iter().filter(|m| m.when == "always") {
        print!("  Scanning {:20} ... ", module.name);
        io::stdout().flush().unwrap_or(());
        if let Some(mount) = scan_module_interactive(module)? {
            println!("{}", if mount.verified { "✅" } else { "⚠️  unverified" });
            mounts.push(mount);
        }
    }
    println!();

    // ── Phase 2: Network modules — one choice covers the whole group
    println!("[ Phase 2: Network support ]");
    println!("  Needed for: DNS resolution, HTTPS, curl, npm install, pip.");
    println!("  Skip if you only run offline tools — fewer mounts is safer.");
    println!();

    if ask_yes_no("  Include network support? (for --network flag)") {
        println!();
        for module in core.modules.iter().filter(|m| m.when == "network") {
            print!("  Scanning {:20} ... ", module.name);
            io::stdout().flush().unwrap_or(());
            if let Some(mount) = scan_module_interactive(module)? {
                println!("{}", if mount.verified { "✅" } else { "⚠️  unverified" });
                mounts.push(mount);
            }
        }
    } else {
        println!("  Skipped. Re-run `cordon scan` later to add network support.");
    }
    println!();

    // ── Phase 3: GUI modules — one choice covers the whole group
    println!("[ Phase 3: GUI support ]");
    println!("  Needed for: X11 apps, Wayland apps, GTK/Qt programs.");
    println!("  Skip if you only run CLI tools.");
    println!();

    if ask_yes_no("  Include GUI support? (for --gui flag)") {
        println!();
        for module in core.modules.iter().filter(|m| m.when == "gui") {
            print!("  Scanning {:20} ... ", module.name);
            io::stdout().flush().unwrap_or(());
            if let Some(mount) = scan_module_interactive(module)? {
                println!("{}", if mount.verified { "✅" } else { "⚠️  unverified" });
                mounts.push(mount);
            }
        }
    } else {
        println!("  Skipped. Re-run `cordon scan` later to add GUI support.");
    }
    println!();

    // ── Phase 4: Optional modules — explained and asked individually
    let optionals: Vec<&CoreModule> = core.modules.iter()
        .filter(|m| m.when == "optional")
        .collect();

    if !optionals.is_empty() {
        println!("[ Phase 4: Optional modules ]");
        println!("  None of these are needed for basic CLI sandbox operation.");
        println!("  Only include what you actually need.\n");

        for module in optionals {
            // Show what this module is and what breaks without it, THEN ask.
            // Learning principle: explain before asking, not after.
            println!("  ┌─ {} ─", module.name);
            println!("  │  What: {}", module.description);
            println!("  │  Without it: {}", module.functionality);

            if ask_yes_no("  └  Include this?") {
                print!("     Scanning ... ");
                io::stdout().flush().unwrap_or(());
                if let Some(mount) = scan_module_interactive(module)? {
                    println!("{}", if mount.verified { "✅" } else { "⚠️  unverified" });
                    mounts.push(mount);
                }
            }
            println!();
        }
    }

    // ── Write system.toml
    let system_config = SystemConfig {
        last_scan: Utc::now().to_rfc3339(),
        cordon_version: env!("CARGO_PKG_VERSION").to_string(),
        mounts,
    };

    crate::config::save_system_config(&system_config)?;
    println!("✅ system.toml written. You can now run: cordon run -- <cmd>");
    println!();
    Ok(())
}

/// Scan a single module interactively.
///
/// If the path is not found and the module is required, prompts the user
/// for a corrected path (e.g. if their distro uses a non-standard layout).
/// If the module is optional and not found, just records it as unverified.
fn scan_module_interactive(module: &CoreModule) -> Result<Option<MountEntry>> {
    let resolved_dir = resolve_env_vars(&module.default_dir);

    let actual_dir = if !Path::new(&resolved_dir).exists() && module.required {
        // Required module not found — ask user for the correct path on this system.
        // This handles non-standard distro layouts (e.g. NixOS, Gentoo).
        match ask_for_path(&module.name, &resolved_dir) {
            Some(corrected) => corrected,
            None => resolved_dir.clone(), // user pressed Enter → record as unverified
        }
    } else {
        resolved_dir.clone()
    };

    scan_module_at(module, &actual_dir)
}

/// Pure scan logic for one module at a specific path — no user interaction.
///
/// Detects whether the path is a symlink or real directory:
///
///   Symlink  → bind_type = "symlink", src = raw link target string.
///              bwrap uses `--symlink <target> <dest>` to recreate it.
///              We store the RAW target (e.g. "usr/bin"), not the resolved path.
///
///   Real dir → bind_type = "ro-bind" or "bind", src = actual path.
///              bwrap uses `--ro-bind <src> <dest>` to mount it.
fn scan_module_at(module: &CoreModule, dir: &str) -> Result<Option<MountEntry>> {
    let path = Path::new(dir);

    // Runtime dirs (XDG_RUNTIME_DIR) use the resolved path as dest too.
    // Everything else maps to its canonical well-known path as dest.
    let dest = if module.default_dir.contains("/run/user/1000") {
        dir.to_string()
    } else {
        module.default_dir.clone()
    };

    if !path.exists() {
        return Ok(Some(MountEntry {
            name: module.name.clone(),
            src: dir.to_string(),
            dest,
            bind_type: if module.mode == "rw" { "bind".to_string() } else { "ro-bind".to_string() },
            mode: module.mode.clone(),
            when: module.when.clone(),
            required: module.required,
            verified: false, // known to be missing — quick check will catch this
        }));
    }

    let metadata = fs::symlink_metadata(path)?;

    // ── SYMLINK: e.g. /bin → usr/bin on merged-usr distros (Ubuntu, Debian)
    if metadata.file_type().is_symlink() {
        let raw_target = fs::read_link(path)?;

        // Resolve the target to verify required_files exist inside it.
        // We only resolve for VERIFICATION — we still store the raw target in system.toml.
        let resolved_target = if raw_target.is_absolute() {
            raw_target.clone()
        } else {
            path.parent().unwrap_or(Path::new("/")).join(&raw_target)
        };

        let verified = module.required_files.iter()
            .all(|f| resolved_target.join(f).exists());

        return Ok(Some(MountEntry {
            name: module.name.clone(),
            src: raw_target.to_string_lossy().to_string(), // raw link target for bwrap
            dest,
            bind_type: "symlink".to_string(),
            mode: module.mode.clone(),
            when: module.when.clone(),
            required: module.required,
            verified,
        }));
    }

    // ── REAL DIRECTORY: e.g. /usr, /etc/ssl/certs on most distros
    let verified = module.required_files.iter()
        .all(|f| path.join(f).exists());

    Ok(Some(MountEntry {
        name: module.name.clone(),
        src: dir.to_string(),
        dest,
        bind_type: if module.mode == "rw" { "bind".to_string() } else { "ro-bind".to_string() },
        mode: module.mode.clone(),
        when: module.when.clone(),
        required: module.required,
        verified,
    }))
}

// ─────────────────────────────────────────────────────────────────
// INTEGRITY CHECK — non-interactive, before every `cordon run`
// ─────────────────────────────────────────────────────────────────

/// Integrity check — runs before every `cordon run`.
///
/// NON-INTERACTIVE. Does NOT write to system.toml under normal conditions.
/// Only checks what is already IN system.toml — nothing more.
///
/// Key design principle:
///   The user chose what to include during the full scan.
///   We do NOT care about modules that are absent from system.toml.
///   We ONLY care that what IS in system.toml still exists on disk.
///
/// Triggers a full scan (with user interaction) when:
///   - system.toml is missing
///   - system.toml is malformed / unreadable
///   - Binary version doesn't match system.toml version
///   - A verified mount's path no longer exists on disk
///
/// Hard blocks (no scan, just error) when:
///   - A foreign entry is found in system.toml
///   - --network is requested but required network modules are absent/unverified
///   - --gui is requested but required GUI modules are absent/unverified
pub fn integrity_check(network: bool, gui: bool) -> Result<SystemConfig> {
    let system_path = crate::config::get_config_dir()?.join("system.toml");

    // ── Step 1: Parse system.toml
    //
    // If it doesn't exist at all: first-time user, run the full scan.
    // If it exists but can't be parsed: something went wrong (hand-edit,
    // partial write, etc.) — safest action is a fresh full scan.
    if !system_path.exists() {
        println!("📝 system.toml not found — running initial scan...");
        full_scan()?;
    }

    let content = fs::read_to_string(&system_path).unwrap_or_default();

    let mut config: SystemConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            println!("⚠️  system.toml is malformed — re-running full scan.");
            println!("   (reason: {})", e);
            full_scan()?;
            let fresh = fs::read_to_string(&system_path)?;
            toml::from_str(&fresh)?
        }
    };

    // ── Step 2: Version check
    //
    // Why: core.toml is compiled into the binary. When Cordon is updated,
    // core.toml may have new modules, removed modules, or renamed fields.
    // A system.toml generated by an older binary may be stale or incompatible.
    // Safest action: trigger a fresh full scan.
    let current_version = env!("CARGO_PKG_VERSION");
    if config.cordon_version != current_version {
        println!(
            "🔄 Cordon updated ({} → {}) — re-running scan for the new version...",
            config.cordon_version, current_version
        );
        full_scan()?;
        let fresh = fs::read_to_string(&system_path)?;
        config = toml::from_str(&fresh)?;
    }

    // ── Step 3: Foreign entry check
    //
    // system.toml is ONLY for entries that correspond to core.toml modules.
    // If there's an entry we don't recognise, something is wrong:
    //   - User hand-edited system.toml and added a custom path
    //   - File was tampered with
    // Either way: hard block. Custom mounts belong in cordon.toml (user.toml).
    let core: CoreConfig = toml::from_str(CORE_TOML)
        .context("Failed to parse embedded core.toml")?;

    let core_names: HashSet<&str> = core.modules.iter()
        .map(|m| m.name.as_str())
        .collect();

    for mount in &config.mounts {
        if !core_names.contains(mount.name.as_str()) {
            bail!(
                "Security: foreign entry '{}' found in system.toml.\n\
                 system.toml must only contain modules defined in core.toml.\n\
                 To add custom paths, use cordon.toml (user.toml) instead.\n\
                 To regenerate a clean system.toml: cordon scan",
                mount.name
            );
        }
    }

    // ── Step 4: File existence check
    //
    // For each VERIFIED mount in system.toml, check that its source path
    // still exists on disk. Paths can break after:
    //   - System upgrades (library paths change)
    //   - Package removal
    //   - Distro migration
    //
    // We skip UNVERIFIED mounts — those were already flagged at scan time
    // and the user was told about them. No point re-checking known-broken paths.
    //
    // On failure: trigger a full scan. The user gets to re-confirm their choices.
    let mut broken: Vec<String> = Vec::new();

    for mount in &config.mounts {
        if !mount.verified {
            continue; // Already known-missing from scan time — skip.
        }

        let exists = if mount.bind_type == "symlink" {
            // Symlink entry: src is the link target (e.g. "usr/bin").
            // We verify the TARGET exists, not the symlink itself.
            // Relative targets (like "usr/bin") are resolved from "/".
            let target = PathBuf::from(&mount.src);
            let resolved = if target.is_absolute() {
                target
            } else {
                PathBuf::from("/").join(&target)
            };
            resolved.exists()
        } else {
            // Real directory: just check the src path exists.
            Path::new(&mount.src).exists()
        };

        if !exists {
            broken.push(mount.name.clone());
        }
    }

    if !broken.is_empty() {
        println!("⚠️  Paths no longer exist: {}", broken.join(", "));
        println!("   System may have changed since last scan — re-running full scan...");
        full_scan()?;
        let fresh = fs::read_to_string(&system_path)?;
        config = toml::from_str(&fresh)?;
    }

    // ── Step 5: Hard fail on --network
    //
    // If the user is requesting network mode, we check whether the required
    // network modules are actually in system.toml AND verified.
    //
    // "Not in system.toml" means the user didn't include network support during
    // the scan — they need to re-scan and answer 'y' to network support.
    //
    // "In system.toml but unverified" means the path existed at scan time but
    // the required files inside it were missing — also needs fixing.
    if network {
        for core_mod in core.modules.iter().filter(|m| m.when == "network" && m.required) {
            match config.mounts.iter().find(|m| m.name == core_mod.name) {
                None => bail!(
                    "❌ --network requires module '{}' but it is not in system.toml.\n\
                     Re-run `cordon scan` and answer 'y' to network support.",
                    core_mod.name
                ),
                Some(m) if !m.verified => bail!(
                    "❌ --network requires module '{}' but it failed verification.\n\
                     Impact: {}\n\
                     Re-run `cordon scan` to fix.",
                    core_mod.name, core_mod.functionality
                ),
                _ => {} // present and verified ✅
            }
        }
    }

    // ── Step 6: Hard fail on --gui
    //
    // Same logic as network. Required GUI modules must be in system.toml
    // and verified. If not, the user needs to re-scan with GUI support enabled.
    if gui {
        for core_mod in core.modules.iter().filter(|m| m.when == "gui" && m.required) {
            match config.mounts.iter().find(|m| m.name == core_mod.name) {
                None => bail!(
                    "❌ --gui requires module '{}' but it is not in system.toml.\n\
                     Re-run `cordon scan` and answer 'y' to GUI support.",
                    core_mod.name
                ),
                Some(m) if !m.verified => bail!(
                    "❌ --gui requires module '{}' but it failed verification.\n\
                     Impact: {}\n\
                     Re-run `cordon scan` to fix.",
                    core_mod.name, core_mod.functionality
                ),
                _ => {} // present and verified ✅
            }
        }
    }

    Ok(config)
}