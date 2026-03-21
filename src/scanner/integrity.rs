use super::CORE_TOML;
use super::full_scan::full_scan;
use crate::config::{CoreConfig, SystemConfig};
use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Integrity check — runs before every `cordon run`.
///
/// NON-INTERACTIVE. Does NOT write to system.toml under any normal condition.
/// Only validates what is already IN system.toml — it never adds new entries.
///
/// Key design principle:
///   The user chose what to include during the full scan.
///   We only care that what IS in system.toml still exists on disk.
///   We never penalise the user for modules they deliberately excluded.
///
/// Triggers a full scan (interactive) when:
///   - system.toml is missing
///   - system.toml is malformed or unreadable
///   - Binary version does not match system.toml cordon_version
///   - A verified mount's source path no longer exists on disk
///
/// Hard-blocks (no scan, immediate error) when:
///   - A foreign entry is found in system.toml (security gate)
///   - A required "always" module is unverified
///   - --network is requested but required network modules are missing/unverified
///   - --gui is requested but required GUI modules are missing/unverified
pub fn integrity_check(network: bool, gui: bool) -> Result<SystemConfig> {
    let system_path = crate::config::get_config_dir()?.join("system.toml");

    // Parse CORE_TOML once — used in multiple checks below.
    let core: CoreConfig =
        toml::from_str(CORE_TOML).context("Failed to parse embedded core.toml")?;

    // ── Step 1: Parse system.toml
    //
    // Missing file  → first-time user, trigger the initial full scan.
    // Malformed file → partial write or hand-edit gone wrong; re-scan is safer
    //                  than operating on a partially-known state.
    if !system_path.exists() {
        println!("📝 system.toml not found — running initial scan...");
        full_scan(None)?;
    }

    let content = fs::read_to_string(&system_path)
        .with_context(|| format!("Failed to read {}", system_path.display()))?;

    let mut config: SystemConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            println!("⚠️  system.toml is malformed — re-running full scan.");
            println!("   Reason: {}", e);
            full_scan(None)?;
            let fresh = fs::read_to_string(&system_path)?;
            toml::from_str(&fresh)
                .context("system.toml is still malformed after re-scan — this is a bug")?
        }
    };

    // ── Step 2: Version check
    //
    // core.toml is compiled into the binary. An update may add, remove, or
    // rename modules. A system.toml from an older binary can be stale or
    // structurally incompatible. Trigger a fresh scan so the user re-confirms.
    let current_version = env!("CARGO_PKG_VERSION");
    if config.cordon_version != current_version {
        println!(
            "🔄 Cordon updated ({} → {}) — re-scanning for new version...",
            config.cordon_version, current_version
        );
        full_scan(None)?;
        let fresh = fs::read_to_string(&system_path)?;
        config = toml::from_str(&fresh)?;
    }

    // ── Step 3: Foreign entry check  (security gate)
    //
    // system.toml is authoritative only for entries that correspond exactly to
    // modules defined in core.toml. An unrecognised name means either:
    //   - Hand-edit / typo by the user
    //   - File was tampered with by a sandboxed process (path-traversal attempt)
    //
    // Hard block and tell the user where custom paths belong (cordon.toml).
    let core_names: HashSet<&str> = core.modules.iter().map(|m| m.name.as_str()).collect();

    for mount in &config.mounts {
        if !core_names.contains(mount.name.as_str()) {
            bail!(
                "Security violation: unknown entry '{}' in system.toml.\n\
                 system.toml must only contain modules defined in core.toml.\n\
                 Add custom paths to cordon.toml instead.\n\
                 To regenerate a clean system.toml: cordon scan",
                mount.name
            );
        }
    }

    // ── Step 4: File existence check
    //
    // For each VERIFIED mount, confirm its source path still exists on disk.
    // Paths can disappear after system upgrades, package removal, or distro
    // migration. We skip UNVERIFIED mounts — they were already flagged at
    // scan time and re-checking them would produce the same result.
    //
    // On failure: trigger a full scan so the user can re-confirm their choices.
    let mut broken: Vec<String> = Vec::new();

    for mount in &config.mounts {
        if !mount.verified {
            continue; // Flagged as missing at scan time — nothing new to report.
        }

        let exists = if mount.bind_type == "symlink" {
            // src is the raw symlink target (e.g. "usr/bin").
            // Relative targets are resolved from "/" (the system root).
            let target = PathBuf::from(&mount.src);
            let resolved = if target.is_absolute() {
                target
            } else {
                PathBuf::from("/").join(&target)
            };
            resolved.exists()
        } else {
            Path::new(&mount.src).exists()
        };

        if !exists {
            broken.push(mount.name.clone());
        }
    }

    if !broken.is_empty() {
        println!("⚠️  Paths no longer exist on disk: {}", broken.join(", "));
        println!("   System may have changed since last scan — re-scanning...");
        full_scan(None)?;
        let fresh = fs::read_to_string(&system_path)?;
        config = toml::from_str(&fresh)?;
    }

    // ── Step 5: Required "always" modules must be verified
    //
    // These are the modules every sandbox run depends on (/usr, /bin, /lib …).
    // If any of them ended up unverified (path never existed or user skipped),
    // there is no point starting bwrap — it will fail immediately. Hard block
    // with a clear message pointing the user at `cordon scan`.
    for core_mod in core
        .modules
        .iter()
        .filter(|m| m.when == "always" && m.required)
    {
        match config.mounts.iter().find(|m| m.name == core_mod.name) {
            None => bail!(
                "Required module '{}' is missing from system.toml.\n\
                 Re-run `cordon scan` to regenerate it.",
                core_mod.name
            ),
            Some(m) if !m.verified => bail!(
                "Required module '{}' failed verification at scan time.\n\
                 Impact: {}\n\
                 Re-run `cordon scan` and provide the correct path when prompted.",
                core_mod.name,
                core_mod.functionality
            ),
            _ => {} // present and verified ✅
        }
    }

    // ── Step 6: Hard fail on --network
    //
    // Required network modules must be present AND verified. If missing, the
    // user needs to re-scan and answer 'y' to network support.
    if network {
        for core_mod in core
            .modules
            .iter()
            .filter(|m| m.when == "network" && m.required)
        {
            match config.mounts.iter().find(|m| m.name == core_mod.name) {
                None => bail!(
                    "--network requires module '{}' but it is absent from system.toml.\n\
                     Re-run `cordon scan` and answer 'y' to network support.",
                    core_mod.name
                ),
                Some(m) if !m.verified => bail!(
                    "--network requires module '{}' but it failed verification.\n\
                     Impact: {}\n\
                     Re-run `cordon scan` to fix.",
                    core_mod.name,
                    core_mod.functionality
                ),
                _ => {}
            }
        }
    }

    // ── Step 7: Hard fail on --gui
    //
    // Same logic as network. Required GUI modules must be present and verified.
    if gui {
        for core_mod in core
            .modules
            .iter()
            .filter(|m| m.when == "gui" && m.required)
        {
            match config.mounts.iter().find(|m| m.name == core_mod.name) {
                None => bail!(
                    "--gui requires module '{}' but it is absent from system.toml.\n\
                     Re-run `cordon scan` and answer 'y' to GUI support.",
                    core_mod.name
                ),
                Some(m) if !m.verified => bail!(
                    "--gui requires module '{}' but it failed verification.\n\
                     Impact: {}\n\
                     Re-run `cordon scan` to fix.",
                    core_mod.name,
                    core_mod.functionality
                ),
                _ => {}
            }
        }
    }

    Ok(config)
}
