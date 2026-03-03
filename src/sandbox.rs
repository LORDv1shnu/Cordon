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
pub fn run_sandboxed(cmd: Vec<String>, network: bool, dry_run: bool) -> Result<()> {
    println!("Checking for Core Dependancy: Bwrap...");
    /// Check bwrap is installed before doing anything else.
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

    let src_dir = project_dir.join("src");
    let has_src = src_dir.exists() && src_dir.is_dir();

    if has_src && !dry_run {
        println!("🔒 Protecting src/ as read-only");
    }

    if !dry_run {
        println!("📂 Project dir: {}", project_path);
    }

    let mut bwrap = Command::new("bwrap");
    
    // Build the args
    let mut args = vec![
        "--unshare-user",
        "--unshare-ipc",
        "--unshare-pid",
        "--unshare-uts",
        "--unshare-cgroup",
        "--ro-bind", "/usr", "/usr",
        "--symlink", "usr/bin", "/bin",
        "--symlink", "usr/lib", "/lib",
        "--symlink", "usr/lib64", "/lib64",
        "--symlink", "usr/sbin", "/sbin",
        "--tmpfs", "/tmp",
        "--proc", "/proc",
        "--dev", "/dev",
        "--bind", project_path, project_path,
    ];

    if has_src {
        let src_path = src_dir.to_str().unwrap();
        args.push("--ro-bind");
        args.push(src_path);
        args.push(src_path);
    }

    if !network {
        args.push("--unshare-net");
        if !dry_run { println!("🌐 Network: disabled"); }
    } else {
        // Instead of exposing all of /etc and /run,
        // only bind the specific files needed for network + DNS + HTTPS.
        // /etc/resolv.conf is a symlink to /run/systemd/resolve/ on systemd systems,
        // so we need both. Exposing all of /etc would leak sensitive files
        // like /etc/passwd, /etc/shadow, /etc/ssh/
    
        // DNS resolution
        if std::path::Path::new("/etc/resolv.conf").exists() {
            bwrap.arg("--ro-bind")
                 .arg("/etc/resolv.conf")
                 .arg("/etc/resolv.conf");
        }
    
        // systemd DNS stub (resolv.conf symlink target)
        if std::path::Path::new("/run/systemd/resolve").exists() {
            bwrap.arg("--ro-bind")
                 .arg("/run/systemd/resolve")
                 .arg("/run/systemd/resolve");
        }
    
        // HTTPS certificates
        if std::path::Path::new("/etc/ssl/certs").exists() {
            bwrap.arg("--ro-bind")
                 .arg("/etc/ssl/certs")
                 .arg("/etc/ssl/certs");
        }
    
        println!("🌐 Network: enabled");
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