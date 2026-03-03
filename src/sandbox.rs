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
pub fn run_sandboxed(cmd: Vec<String>, network: bool, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("🏗️ DRY RUN: The following bwrap command would be executed:");
    } else {
        println!("🔒 Running inside sandbox...");
    }

    let project_dir = env::current_dir()?;
    let project_path = project_dir.to_str().unwrap();

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
        args.push("--ro-bind"); args.push("/etc"); args.push("/etc");
        args.push("--ro-bind"); args.push("/run"); args.push("/run");
        if !dry_run { println!("🌐 Network: enabled"); }
    }

    args.push("--chdir");
    args.push(project_path);
    args.push("--");

    if dry_run {
        print!("bwrap ");
        for arg in &args {
            print!("{} ", arg);
        }
        for arg in &cmd {
            print!("{} ", arg);
        }
        println!();
        return Ok(());
    }

    bwrap.args(&args);
    bwrap.args(&cmd);

    let status = bwrap.status()?;

    if status.success() {
        println!("✅ Command completed successfully");
    } else {
        println!("❌ Command failed with status: {}", status);
    }

    Ok(())
}
