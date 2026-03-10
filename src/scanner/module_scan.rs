use super::env_resolver::{
    resolve_dbus_socket, resolve_env_vars, resolve_pipewire_socket, resolve_pulse_socket,
};
use crate::config::{CoreModule, MountEntry};
use anyhow::Result;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// When a required module is not found at its default path, ask the user
/// for a corrected path. Returns `None` if the user presses Enter (skips).
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
/// Special case: `dbus_session` resolves its socket via `$DBUS_SESSION_BUS_ADDRESS`
/// (or `$XDG_RUNTIME_DIR/bus` as fallback) instead of the generic env-var resolver.
/// This handles non-standard D-Bus configurations on some distros.
///
/// For all other modules, if the path is not found and the module is `required`,
/// the user is prompted for a corrected path (handles non-standard distro layouts
/// such as NixOS or Gentoo). Optional modules that cannot be found are recorded
/// as unverified without prompting.
pub fn scan_module_interactive(module: &CoreModule) -> Result<Option<MountEntry>> {
    // D-Bus session socket gets special resolution from DBUS_SESSION_BUS_ADDRESS.
    if module.name == "dbus_session" {
        if let Some(socket_path) = resolve_dbus_socket() {
            // The socket is a file, not a directory. Derive the parent directory as
            // the mount source so bwrap exposes the whole runtime socket namespace.
            let parent = Path::new(&socket_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| socket_path.clone());
            return scan_module_at(module, &parent);
        }
        // Not found — fall through to scan_module_at with env-var resolved path.
    } else if module.name == "audio_pipewire" {
        if let Some(socket_path) = resolve_pipewire_socket() {
            // It's a file but module definition expects it to mount the directory or the file?
            // "audio_pipewire" in core.toml:
            // default_dir    = "/run/user/1000/pipewire-0"
            // required_files = []
            // So we can just mount the socket itself, similar to the fallback path or we can mount the parent directory like dbus_session?
            // Actually, core.toml specifies `default_dir = "/run/user/1000/pipewire-0"` which is the socket itself.
            // If the user's `$PIPEWIRE_RUNTIME_DIR` is different, let's just make `scan_module_at` use the socket directly.
            // But let's check core.toml definition. dbus_session mounts the *parent* dir because it specifies `$XDG_RUNTIME_DIR` as `default_dir` and `bus` as `required_files`!
            // Wait, for `audio_pipewire`: `default_dir` is the socket path directly (no required files). So we pass the exact socket file path as `actual_dir`.

            return scan_module_at(module, &socket_path);
        }
    } else if module.name == "audio_pulse"
        && let Some(socket_path) = resolve_pulse_socket()
    {
        // "audio_pulse" in core.toml:
        // default_dir = "/run/user/1000/pulse"
        // required_files = ["native"]
        // This means `socket_path` (which points to `native`) is inside the dir we should mount.
        let parent = Path::new(&socket_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| socket_path.clone());
        return scan_module_at(module, &parent);
    }

    let resolved_dir = resolve_env_vars(&module.default_dir);

    let actual_dir = if !Path::new(&resolved_dir).exists() && module.required {
        // Required module not found — ask user for the correct path.
        match ask_for_path(&module.name, &resolved_dir) {
            Some(corrected) => corrected,
            None => resolved_dir.clone(), // user skipped → record as unverified
        }
    } else {
        resolved_dir.clone()
    };

    scan_module_at(module, &actual_dir)
}

/// Pure scan logic for a single module at a known path; no user interaction.
///
/// Detects whether the path is a symlink or a real directory and records
/// the correct `bind_type` for bwrap:
///
///   Symlink  → `bind_type = "symlink"`, `src` = raw link target.
///              bwrap reconstructs it with `--symlink <target> <dest>`.
///              The raw target (e.g. `usr/bin`) is stored — not the resolved path.
///
///   Real dir → `bind_type = "ro-bind"` (or `"bind"` for rw modules),
///              `src` = absolute path to the directory.
pub fn scan_module_at(module: &CoreModule, dir: &str) -> Result<Option<MountEntry>> {
    let path = Path::new(dir);

    // XDG_RUNTIME_DIR-based mounts use the resolved path as their dest too,
    // because their canonical name varies per user. Everything else is mounted
    // at its well-known system path.
    let dest = if module.default_dir.contains("/run/user/1000") {
        dir.to_string()
    } else {
        module.default_dir.clone()
    };

    // Path does not exist on this system — record as unverified so that
    // integrity_check can report it cleanly rather than crashing at run time.
    if !path.exists() {
        return Ok(Some(MountEntry {
            name: module.name.clone(),
            src: dir.to_string(),
            dest,
            bind_type: if module.name == "gpu_dri" {
                "dev-bind".to_string()
            } else if module.mode == "rw" {
                "bind".to_string()
            } else {
                "ro-bind".to_string()
            },
            mode: module.mode.clone(),
            when: module.when.clone(),
            required: module.required,
            verified: false,
        }));
    }

    let metadata = fs::symlink_metadata(path)?;

    // ── SYMLINK (e.g. /bin → usr/bin on merged-usr distros)
    if metadata.file_type().is_symlink() {
        let raw_target = fs::read_link(path)?;

        // Resolve the target only to check whether required_files exist inside it.
        // We still store the raw target so bwrap can recreate the symlink correctly.
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
            src: raw_target.to_string_lossy().to_string(),
            dest,
            bind_type: "symlink".to_string(),
            mode: module.mode.clone(),
            when: module.when.clone(),
            required: module.required,
            verified,
        }));
    }

    // ── REAL DIRECTORY (e.g. /usr, /etc/ssl/certs)
    let verified = module.required_files.iter().all(|f| path.join(f).exists());

    Ok(Some(MountEntry {
        name: module.name.clone(),
        src: dir.to_string(),
        dest,
        bind_type: if module.name == "gpu_dri" {
            "dev-bind".to_string()
        } else if module.mode == "rw" {
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
