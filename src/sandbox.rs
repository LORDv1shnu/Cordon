use anyhow::Result;
use std::process::Command;
use std::env;

/// Runs a command inside a bubblewrap (bwrap) sandbox.
pub fn run_sandboxed(cmd: Vec<String>, network: bool) -> Result<()> {
    // Phase 2: ensure system.toml resides and is valid
    let system_config = crate::scanner::pre_flight_check()?;
    let user_config = crate::config::find_user_config()?.unwrap_or_default();

    let project_dir = env::current_dir()?;
    let project_path = project_dir.to_str().unwrap();

    // Detect src/ for read-only overlay protection
    let src_dir = project_dir.join("src");
    let has_src = src_dir.exists() && src_dir.is_dir();

    println!("🔒 Sandbox starting...");

    let mut bwrap = Command::new("bwrap");
    bwrap
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
                bwrap.arg("--symlink").arg(&mount.src).arg(&mount.dest);
            }
            "ro-bind" => {
                if mount.verified {
                    bwrap.arg("--ro-bind").arg(&mount.src).arg(&mount.dest);
                }
            }
            _ => {
                bwrap.arg("--ro-bind").arg(&mount.src).arg(&mount.dest);
            }
        }
    }

    // Default virtual filesystems
    bwrap
        .arg("--tmpfs").arg("/tmp")
        .arg("--proc").arg("/proc")
        .arg("--dev").arg("/dev");

    // Project bind (writable)
    bwrap.arg("--bind").arg(project_path).arg(project_path);

    // Overlay src/ as read-only if it exists
    if has_src {
        let src_path = src_dir.to_str().unwrap();
        bwrap.arg("--ro-bind").arg(src_path).arg(src_path);
    }

    // Process user mounts from cordon.toml
    for mount in user_config.mounts {
        let mode_flag = if mount.mode == "rw" { "--bind" } else { "--ro-bind" };
        bwrap.arg(mode_flag).arg(&mount.src).arg(&mount.dest);
    }

    if !network {
        bwrap.arg("--unshare-net");
        println!("🌐 Network: disabled");
    } else {
        println!("🌐 Network: enabled");
    }

    bwrap
        .arg("--chdir").arg(project_path)
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
