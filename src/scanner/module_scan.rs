use super::env_resolver::resolve_env_vars;
use crate::config::{CoreModule, MountEntry};
use anyhow::Result;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// When a required module is not found at its default path, ask the user
/// for a corrected path. Returns None if the user presses Enter (skips it).
fn ask_for_path(module_name: &str, tried_path: &str) -> Option<String> {
    println!();
    println!("     Not found at: {}", tried_path);
    println!(
        "     Enter a corrected path for '{}', or press Enter to skip:",
        module_name
    );
    print!("     > ");
    io::stdout().flush().unwrap_or(());
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap_or(0);
    let trimmed = input.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Scan a single module interactively.
///
/// If the path is not found and the module is required, prompts the user
/// for a corrected path (e.g. if their distro uses a non-standard layout).
/// If the module is optional and not found, just records it as unverified.
pub fn scan_module_interactive(module: &CoreModule) -> Result<Option<MountEntry>> {
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
pub fn scan_module_at(module: &CoreModule, dir: &str) -> Result<Option<MountEntry>> {
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
            bind_type: if module.mode == "rw" {
                "bind".to_string()
            } else {
                "ro-bind".to_string()
            },
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

        let verified = module
            .required_files
            .iter()
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
    let verified = module.required_files.iter().all(|f| path.join(f).exists());

    Ok(Some(MountEntry {
        name: module.name.clone(),
        src: dir.to_string(),
        dest,
        bind_type: if module.mode == "rw" {
            "bind".to_string()
        } else {
            "ro-bind".to_string()
        },
        mode: module.mode.clone(),
        when: module.when.clone(),
        required: module.required,
        verified,
    }))
}
