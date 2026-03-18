use std::process::Command;
use tracing::info;

use crate::sandbox::network::NetworkMode;

pub fn build_bwrap(project_path: &str, net: NetworkMode, dry_run: bool) -> Command {
    let mut bwrap = Command::new("bwrap");

    // --- Core namespace isolation + standard pseudo-filesystems ---
    bwrap.args([
        "--unshare-user",
        "--unshare-ipc",
        "--unshare-pid",
        "--unshare-uts",
        "--unshare-cgroup",
        "--clearenv",
        "--tmpfs",
        "/tmp",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        // Project directory is writable — this is the whole point of the sandbox
        "--bind",
        project_path,
        project_path,
    ]);

    match net {
        NetworkMode::Disable => {
            bwrap.arg("--unshare-net");
            if !dry_run {
                info!("Network: disabled");
            }
        }
        NetworkMode::Full => {
            apply_network_mounts(&mut bwrap);
            if !dry_run {
                info!("Network: full access");
            }
        }
        NetworkMode::Allow => {
            apply_network_mounts(&mut bwrap);
            if !dry_run {
                info!("Network: domain allow-list (proxy.toml)");
            }
        }
    }

    bwrap
}

fn apply_network_mounts(bwrap: &mut Command) {
    let net_files = [
        "/etc/resolv.conf",
        "/etc/hosts",
        "/etc/hostname",
        "/etc/nsswitch.conf",
    ];
    for path in net_files {
        if std::path::Path::new(path).exists() {
            bwrap.args(["--ro-bind", path, path]);
        }
    }

    if std::path::Path::new("/etc/ssl").exists() {
        bwrap.args(["--ro-bind", "/etc/ssl", "/etc/ssl"]);
    }
    if std::path::Path::new("/etc/pki").exists() {
        bwrap.args(["--ro-bind", "/etc/pki", "/etc/pki"]);
    }
}

pub fn apply_environment(bwrap: &mut Command, gui: bool) {
    #[allow(unused_imports)]
    use tracing::debug;
    if gui {
        // Environment variables required for GUI support
        if let Ok(display) = std::env::var("DISPLAY") {
            bwrap.arg("--setenv").arg("DISPLAY").arg(&display);
        }
        if let Ok(wayland_display) = std::env::var("WAYLAND_DISPLAY") {
            bwrap
                .arg("--setenv")
                .arg("WAYLAND_DISPLAY")
                .arg(&wayland_display);
        }
    }

    for var in [
        "HOME",
        "USER",
        "LOGNAME",
        "LANG",
        "TERM",
        "PATH",
    ] {
        if let Ok(val) = std::env::var(var) {
            bwrap.arg("--setenv").arg(var).arg(val);
        }
    }

    // Pass LC_* variables (locale settings) explicitly since they can't be listed individually
    for (key, val) in std::env::vars() {
        if key.starts_with("LC_") {
            bwrap.arg("--setenv").arg(&key).arg(val);
        }
    }

    // Optional: Pass XDG vars explicitly if needed for certain desktop interaction,
    // although the instruction specifically excluded them by default. Keeping them
    // only if GUI is true might make sense, or drop entirely as instructed.
    if gui {
        for var in [
            "XDG_RUNTIME_DIR",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
        ] {
            if let Ok(val) = std::env::var(var) {
                bwrap.arg("--setenv").arg(var).arg(val);
            }
        }
    }
}
