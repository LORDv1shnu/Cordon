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
//!
//! ## Exit code contract
//! Non-zero exits from the sandboxed process are forwarded via an anyhow error
//! encoded as `"exit code: N"`. `main.rs` decodes this and calls
//! `std::process::exit(N)` so the shell sees the child's real exit code.
//! Sandbox setup failures (bwrap missing, scan error, etc.) produce exit 125.

use anyhow::{bail, Result};
use std::process::Command;
use std::env;
use std::path::PathBuf;

/// Builds and executes the bubblewrap sandbox.
///
/// Flow:
///   1. Check bwrap is installed
///   2. Run integrity_check() to validate system.toml
///   3. Build bwrap command: namespace isolation, system mounts, user mounts
///   4. Prompt user before applying cordon.toml (user.toml) mounts
///   5. Forward safe environment variables into sandbox
///   6. Execute command (or print in dry-run mode)
///
/// All mount paths come from system.toml and cordon.toml — nothing is hardcoded.


pub fn run_sandboxed(cmd: Vec<String>, network: bool, dry_run: bool, gui: bool, optional: Vec<String>) -> Result<()> {
    println!("Checking for Core Dependancy: Bwrap...");
    // Check bwrap is installed before doing anything else.
    // If missing, exit 125 (sandbox setup failed — matches bwrap/shell convention).
    if std::process::Command::new("which")
        .arg("bwrap")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!(
            "error: bubblewrap (bwrap) is not installed or not found in PATH.\n\
             Install it with:\n  \
               Ubuntu/Debian:  sudo apt install bubblewrap\n  \
               Arch:           sudo pacman -S bubblewrap\n  \
               Fedora:         sudo dnf install bubblewrap"
        );
        bail!("exit code: 125");
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

    let system_config = crate::scanner::integrity_check(network, gui)?;

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
        if mount.when == "optional" {
            if !optional.contains(&mount.name) { continue; }
            if !mount.verified {
                eprintln!("warning: --opt-in {} requested but module is unverified — skipping", mount.name);
                continue;
            }
        }

        let arg_flag = format!("--{}", mount.bind_type);
        bwrap.arg(&arg_flag).arg(&mount.src).arg(&mount.dest);
    }

    // --- Apply dynamic mounts from user.toml (with confirmation) ---
    if let Ok(Some(ref user_config)) = crate::config::find_user_config() {
        let apply = if dry_run {
            // In dry-run mode, always include user.toml mounts so the full command is visible
            true
        } else {
            // Ask user before exposing anything from cordon.toml
            println!();
            println!("⚠️  cordon.toml found with custom path exposures.");
            loop {
                print!("   Apply these mounts? [Enter=yes / N=no / D=show paths]: ");
                std::io::Write::flush(&mut std::io::stdout()).unwrap_or(());
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).unwrap_or(0);
                match input.trim().to_uppercase().as_str() {
                    "" | "Y" => break true,
                    "N" => {
                        println!("   Skipping cordon.toml mounts.");
                        break false;
                    }
                    "D" => {
                        println!("   Paths in cordon.toml:");
                        for m in &user_config.mounts {
                            println!("     {} {} ({})", if m.mode == "rw" { "rw" } else { "ro" }, m.src, m.dest);
                        }
                        // loop again to ask
                    }
                    _ => {
                        println!("   Unknown input. Enter, N, or D.");
                    }
                }
            }
        };

        if apply {
            for mount in &user_config.mounts {
                let arg_flag = if mount.mode == "rw" { "--bind" } else { "--ro-bind" };
                bwrap.arg(arg_flag).arg(&mount.src).arg(&mount.dest);
            }
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

    for var in [
        "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "PATH", "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"
    ] {
        if let Ok(val) = std::env::var(var) {
            bwrap.arg("--setenv").arg(var).arg(val);
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
        Ok(())
    } else {
        // Extract the child's exit code and propagate it via an encoded error.
        // main.rs decodes "exit code: N" and calls std::process::exit(N).
        let code = status.code().unwrap_or(1);
        eprintln!("❌ Command exited with status: {}", code);
        bail!("exit code: {}", code);
    }
}