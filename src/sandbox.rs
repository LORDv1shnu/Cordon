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
/// # Security Notes
/// - System mounts must already be verified by scanner
/// - src/ is protected via read-only overlay
/// - Network is disabled by default
pub fn run_sandboxed(cmd: Vec<String>, network: bool) -> Result<()> {
    // Phase 2: ensure system.toml resides and is valid
    let system_config = crate::scanner::pre_flight_check()?;
    let user_config = crate::config::find_user_config()?.unwrap_or_default();

    let project_dir: PathBuf = env::current_dir()?;

    // Detect src/ for read-only overlay protection
    let src_dir = project_dir.join("src");
    let has_src = src_dir.exists() && src_dir.is_dir();

    println!("🔒 Sandbox starting...");

    let mut bwrap = Command::new("bwrap");
    bwrap
        .arg("--die-with-parent") // prevent orphan sandbox processes
        .arg("--unshare-user")
        .arg("--unshare-ipc")
        .arg("--unshare-pid")
        .arg("--unshare-uts")
        .arg("--unshare-cgroup");

    // Process system mounts from system.toml
    for mount in system_config.mounts {
        // Skip network-only mounts if network is disabled
        if mount.when == "network" && !network {
            continue;
        }

        match mount.bind_type.as_str() {
            "symlink" => {
                bwrap.arg("--symlink")
                    .arg(&mount.src)
                    .arg(&mount.dest);
            }
            "ro-bind" => {
                if mount.verified {
                    bwrap.arg("--ro-bind")
                        .arg(&mount.src)
                        .arg(&mount.dest);
                }
            }
            _ => {
                bwrap.arg("--ro-bind")
                    .arg(&mount.src)
                    .arg(&mount.dest);
            }
        }
    }

    // Minimal required virtual filesystems
    bwrap
        .arg("--tmpfs").arg("/tmp")
        .arg("--proc").arg("/proc")
        .arg("--dev").arg("/dev");

    // Bind the project directory as writable
    bwrap
        .arg("--bind")
        .arg(&project_dir)
        .arg(&project_dir);

    // Overlay src/ as read-only if it exists
    // Later mount wins — this protects source code
    if has_src {
        bwrap
            .arg("--ro-bind")
            .arg(&src_dir)
            .arg(&src_dir);
    }

    // Process user mounts from cordon.toml
    for mount in user_config.mounts {
        let mode_flag = if mount.mode == "rw" {
            "--bind"
        } else {
            "--ro-bind"
        };

        bwrap
            .arg(mode_flag)
            .arg(&mount.src)
            .arg(&mount.dest);
    }

    // Network handling
    if !network {
        bwrap.arg("--unshare-net");
        println!("🌐 Network: disabled");
    } else {
        println!("🌐 Network: enabled");
    }

    bwrap
        .arg("--chdir").arg(&project_dir)
        .arg("--") // end of bwrap args
        .args(&cmd);

    let status = bwrap.status()?;

    if status.success() {
        println!("✅ Command completed successfully");
    } else {
        println!("❌ Command failed with status: {}", status);
    }

    Ok(())
}