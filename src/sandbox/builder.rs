use std::process::Command;

pub fn build_bwrap(project_path: &str, network: bool, dry_run: bool) -> Command {
    let mut bwrap = Command::new("bwrap");

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

    bwrap
}

pub fn apply_environment(bwrap: &mut Command, gui: bool) {
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
}
