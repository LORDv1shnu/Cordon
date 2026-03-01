use anyhow::Result;
use std::process::Command;
use std::env;

/// Runs a command inside a bubblewrap (bwrap) sandbox.
///
/// # What this does
/// Builds a bwrap invocation that:
/// - Creates isolated namespaces (user, pid, ipc, uts, cgroup)
/// - Mounts system directories read-only
/// - Binds the current project directory as writable
/// - Overlays src/ as read-only if it exists
/// - Optionally isolates or allows network
///
/// # Phase 2 note
/// Currently the bwrap arguments are hardcoded.
/// In Phase 2 this will be replaced by reading verified paths from
/// ~/.config/cordon/system.toml and the per-project cordon.toml (user.toml),
/// with symlink vs ro-bind chosen per entry's `bind_type` field.
pub fn run_sandboxed(cmd: Vec<String>, network: bool) -> Result<()> {
    println!("🔒 Running inside sandbox...");

    let project_dir = env::current_dir()?;
    let project_path = project_dir.to_str().unwrap();

    // Detect src/ — if present it will be overlaid as read-only
    // This protects source code even though the rest of the project is writable
    let src_dir = project_dir.join("src");
    let has_src = src_dir.exists() && src_dir.is_dir();

    if has_src {
        println!("🔒 Protecting src/ as read-only");
    }

    println!("📂 Project dir: {}", project_path);

    let mut bwrap = Command::new("bwrap");
    bwrap
        // Isolate all namespaces except network (handled separately below)
        .arg("--unshare-user")   // new user namespace (no root required)
        .arg("--unshare-ipc")    // isolate IPC
        .arg("--unshare-pid")    // isolate process IDs
        .arg("--unshare-uts")    // isolate hostname
        .arg("--unshare-cgroup") // isolate cgroup

        // Mount /usr read-only — contains all binaries and libraries on modern distros
        .arg("--ro-bind").arg("/usr").arg("/usr")

        // Recreate merged-usr symlinks inside the sandbox.
        // On Ubuntu/Debian and most modern distros, /bin /lib /sbin are symlinks
        // into /usr. We must recreate them or executables won't be found.
        // Phase 2: scanner will detect these at scan time and store bind_type
        // in system.toml so this doesn't need to be hardcoded here.
        .arg("--symlink").arg("usr/bin").arg("/bin")
        .arg("--symlink").arg("usr/lib").arg("/lib")
        .arg("--symlink").arg("usr/lib64").arg("/lib64")
        .arg("--symlink").arg("usr/sbin").arg("/sbin")

        // Minimal required virtual filesystems
        .arg("--tmpfs").arg("/tmp")   // writable temp space
        .arg("--proc").arg("/proc")   // process info (needed by many programs)
        .arg("--dev").arg("/dev")     // device nodes

        // Bind the project directory as writable.
        // This is the only writable real path in the sandbox.
        .arg("--bind").arg(project_path).arg(project_path);

    // Overlay src/ as read-only on top of the writable project bind.
    // bwrap processes mounts in order — the later ro-bind on src/ wins,
    // making it read-only even though the parent directory is writable.
    if has_src {
        let src_path = src_dir.to_str().unwrap();
        bwrap.arg("--ro-bind").arg(src_path).arg(src_path);
    }

    // Network isolation.
    // Default: --unshare-net fully removes network access.
    // When --network is passed: we must mount /etc and /run read-only.
    //   - /etc/resolv.conf is needed for DNS.
    //   - On systemd systems, /etc/resolv.conf is a symlink pointing to
    //     /run/systemd/resolve/stub-resolv.conf, so /run must also be mounted.
    //   - Exposing all of /etc is intentionally broad here (Phase 2 will
    //     narrow this to only required files via system.toml).
    if !network {
        bwrap.arg("--unshare-net");
        println!("🌐 Network: disabled");
    } else {
        bwrap
            .arg("--ro-bind").arg("/etc").arg("/etc")
            .arg("--ro-bind").arg("/run").arg("/run");
        println!("🌐 Network: enabled");
    }

    // Set working directory inside sandbox to match outside
    bwrap
        .arg("--chdir").arg(project_path)
        .arg("--"); // separator: everything after this is the user's command

    bwrap.args(&cmd);

    let status = bwrap.status()?;

    if status.success() {
        println!("✅ Command completed successfully");
    } else {
        println!("❌ Command failed with status: {}", status);
    }

    Ok(())
}
