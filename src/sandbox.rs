//! sandbox.rs
//!
//! Responsible for building and executing the bubblewrap sandbox.
//!
//! This module:
//! - Loads verified system mounts from system.toml
//! - Loads optional per-project user mounts from cordon.toml
//! - Constructs the final bwrap command
//! - Applies namespace isolation
//! - Applies network policy
//! - Executes the requested command inside the sandbox
//!
//! This module contains NO scanning logic and NO configuration mutation logic.
//! It only consumes already-verified configuration.

use anyhow::Result;
use std::process::Command;
use std::env;
use std::path::PathBuf;

/// Runs a command inside a bubblewrap (bwrap) sandbox.
///
/// # Responsibilities
/// - Ensures system.toml exists (via pre_flight_check)
/// - Applies system mounts (ro-bind / symlink)
/// - Applies project and user mounts
/// - Handles network isolation
/// - Executes the final command
///
/// # Phase 2 note
/// Currently the bwrap arguments are hardcoded.
/// In Phase 2 this will be replaced by reading verified paths from
/// ~/.config/cordon/system.toml and the per-project cordon.toml (user.toml),
/// with symlink vs ro-bind chosen per entry's `bind_type` field.
pub fn run_sandboxed(cmd: Vec<String>, network: bool, dry_run: bool, gui: bool) -> Result<()> {
    println!("Checking for Core Dependancy: Bwrap...");
    // Check bwrap is installed before doing anything else.
    if std::process::Command::new("which")
        .arg("bwrap")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        anyhow::bail!(
            "bubblewrap (bwrap) is not installed or not found in PATH.\n\
             Install it with:\n\
               Ubuntu/Debian:  sudo apt install bubblewrap\n\
               Arch:           sudo pacman -S bubblewrap\n\
               Fedora:         sudo dnf install bubblewrap"
        );
    }

    println!("🔒 Running inside sandbox...");

    let project_dir: PathBuf = env::current_dir()?;
    let project_path = project_dir.to_str().unwrap();
    let src_dir = project_dir.join("src");
    let has_src = src_dir.exists() && src_dir.is_dir();

    if has_src && !dry_run {
        println!("🔒 Protecting src/ as read-only");
    }

    if !dry_run {
        println!("📂 Project dir: {}", project_dir.display());
    }

    let mut bwrap = Command::new("bwrap");

    let system_config = crate::scanner::pre_flight_check(network, gui)?;

    // --- Core namespace isolation + standard pseudo-filesystems ---
    bwrap.args([
        "--unshare-user",
        "--unshare-ipc",
        "--unshare-pid",
        "--unshare-uts",
        "--unshare-cgroup",
        "--tmpfs", "/tmp",
        "--proc", "/proc",
        "--dev",  "/dev",
        // Project directory is writable — this is the whole point of the sandbox
        "--bind", project_path, project_path,
    ]);

    if !network {
        bwrap.arg("--unshare-net");
        if !dry_run { println!("🌐 Network: disabled"); }
    } else {
        println!("🌐 Network: enabled");
    }

    // --- Apply dynamic mounts from system.toml ---
    for mount in system_config.mounts {
        if !mount.verified { continue; } // skip unverified modules
        if mount.when == "network" && !network { continue; }
        if mount.when == "gui" && !gui { continue; }
        if mount.when == "optional" { continue; } // Optional mounts from system.toml not explicitly enabled yet

        let arg_flag = format!("--{}", mount.bind_type);
        bwrap.arg(&arg_flag).arg(&mount.src).arg(&mount.dest);
    }

    // --- Apply dynamic mounts from user.toml ---
    if let Ok(Some(user_config)) = crate::config::find_user_config() {
        for mount in user_config.mounts {
            let arg_flag = if mount.mode == "rw" { "--bind" } else { "--ro-bind" };
            bwrap.arg(arg_flag).arg(&mount.src).arg(&mount.dest);
        }
    }

    if has_src {
        let src_path = src_dir.to_str().unwrap();
        bwrap.arg("--ro-bind");
        bwrap.arg(src_path);
        bwrap.arg(src_path);
    }

    if gui {
        // Environment variables required for GUI support
        if let Ok(display) = std::env::var("DISPLAY") {
            bwrap.arg("--setenv").arg("DISPLAY").arg(&display);
        }
        if let Ok(wayland_display) = std::env::var("WAYLAND_DISPLAY") {
            bwrap.arg("--setenv").arg("WAYLAND_DISPLAY").arg(&wayland_display);
        }
    }

    bwrap
        .arg("--chdir").arg(&project_dir)
        .arg("--") // end of bwrap args
        .args(&cmd);

    if dry_run {
        let program = bwrap.get_program().to_string_lossy();
        let args = bwrap
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
    
        println!("🧪 Dry run mode: command not executed");
        println!("{} {}", program, args);
        return Ok(());
    }

    let status = bwrap.status()?;

    if status.success() {
        println!("✅ Command completed successfully");
    } else {
        println!("❌ Command failed with status: {}", status);
    }

    Ok(())
}