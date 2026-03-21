use anyhow::Result;
use std::process::Command;
use std::path::Path;

pub fn run_doctor() -> Result<()> {
    println!();
    println!("🩺 Cordon Doctor");
    println!("=================");
    println!();

    // 1. Kernel version (`uname -r`)
    let kernel_ver = Command::new("uname").arg("-r").output();
    if let Ok(out) = kernel_ver {
        let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
        println!("[PASS] Kernel version: {}", ver);
    } else {
        println!("[FAIL] Kernel version: could not run uname");
    }

    // 2. bwrap version
    let bwrap_ver = Command::new("bwrap").arg("--version").output();
    if let Ok(out) = bwrap_ver {
        let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
        println!("[PASS] Bubblewrap: {}", ver);
    } else {
        println!("[FAIL] Bubblewrap: not found. Hint: sudo apt install bubblewrap");
    }

    // 3. User namespaces (`/proc/sys/kernel/unprivileged_userns_clone`)
    let userns = std::fs::read_to_string("/proc/sys/kernel/unprivileged_userns_clone");
    if let Ok(val) = userns {
        if val.trim() == "1" {
            println!("[PASS] User namespaces: enabled");
        } else {
            println!("[FAIL] User namespaces: disabled. Hint: sudo sysctl -w kernel.unprivileged_userns_clone=1");
        }
    } else {
        println!("[WARN] User namespaces: /proc/sys/kernel/unprivileged_userns_clone not found");
    }

    // 4. AppArmor userns restriction
    let apparmor = std::fs::read_to_string("/proc/sys/kernel/apparmor_restrict_unprivileged_userns");
    if let Ok(val) = apparmor {
        if val.trim() == "0" {
            println!("[PASS] AppArmor userns: unrestricted");
        } else {
            println!("[FAIL] AppArmor userns: restricted. Hint: sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0");
        }
    } else {
        println!("[PASS] AppArmor userns: no restriction found");
    }

    // 5. Available namespace types
    if Path::new("/proc/self/ns").exists() {
        println!("[PASS] Namespace types: accessible");
    } else {
        println!("[FAIL] Namespace types: /proc/self/ns not accessible");
    }

    // 6. Docker-in-Docker
    if Path::new("/.dockerenv").exists() {
        println!("[WARN] Environment: Running inside Docker. Sandboxing may be restricted.");
    } else {
        println!("[PASS] Environment: Host or VM (not Docker)");
    }

    // 7. WSL2 detection
    let proc_version = std::fs::read_to_string("/proc/version").unwrap_or_default();
    if proc_version.to_lowercase().contains("microsoft") {
        println!("[WARN] Environment: WSL2 detected. May limit namespaces.");
    }

    // 8. Flatpak detection
    if std::env::var("FLATPAK_ID").is_ok() {
        println!("[WARN] Environment: Flatpak restricted environment.");
    }

    // 9. strace availability
    let strace = Command::new("strace").arg("-V").output();
    if strace.is_ok() {
        println!("[PASS] strace: installed");
    } else {
        println!("[WARN] strace: not found. Hint: sudo apt install strace");
    }

    // 10. system.toml check
    if crate::config::get_config_dir().map(|d| d.join("system.toml").exists()).unwrap_or(false) {
        println!("[PASS] Config: system.toml initialized");
    } else {
        println!("[FAIL] Config: system.toml not found. Hint: cordon scan");
    }

    println!();
    Ok(())
}
