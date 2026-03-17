use crate::sandbox::builder::{apply_environment, build_bwrap};
use crate::sandbox::mounts::{apply_system_mounts, apply_user_mounts};
use crate::scanner::integrity_check;
use anyhow::{Result, bail};
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
use crate::sandbox::network::NetworkMode;
use crate::sandbox::proxy::ProxyHandle;

pub fn run_sandboxed(
    cmd: Vec<String>,
    net: NetworkMode,
    domains: Vec<String>,
    dry_run: bool,
    gui: bool,
    optional: Vec<String>,
) -> Result<()> {
    println!("Checking for core dependency: bwrap...");
    // Verify bwrap is installed before doing anything else.
    // Failure here exits 125, matching bwrap's own convention for setup errors.
    if std::process::Command::new("bwrap")
        .arg("--version")
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

    let needs_net_mounts = net != NetworkMode::Disable;
    let system_config = integrity_check(needs_net_mounts, gui)?;

    let mut bwrap = build_bwrap(project_path, net, dry_run);

    apply_system_mounts(&mut bwrap, &system_config, needs_net_mounts, gui, &optional);
    apply_user_mounts(&mut bwrap, dry_run);

    if has_src {
        let src_path = src_dir.to_str().unwrap();
        bwrap.arg("--ro-bind");
        bwrap.arg(src_path);
        bwrap.arg(src_path);
    }

    apply_environment(&mut bwrap, gui);

    // --- Proxy Setup ---
    let _proxy: Option<ProxyHandle> = if net == NetworkMode::Allow {
        let proxy_cfg = crate::sandbox::proxy::load_config(&project_dir);
        let mut final_domains = domains.clone();
        final_domains.extend(proxy_cfg.domains);
        final_domains.sort();
        final_domains.dedup();

        match ProxyHandle::spawn(final_domains.clone()) {
            Ok(p) => {
                let proxy_url = format!("http://127.0.0.1:{}", p.port);
                bwrap.arg("--setenv").arg("HTTP_PROXY").arg(&proxy_url);
                bwrap.arg("--setenv").arg("HTTPS_PROXY").arg(&proxy_url);
                bwrap.arg("--setenv").arg("http_proxy").arg(&proxy_url);
                bwrap.arg("--setenv").arg("https_proxy").arg(&proxy_url);
                bwrap.arg("--setenv").arg("ALL_PROXY").arg(&proxy_url);
                bwrap.arg("--setenv").arg("all_proxy").arg(&proxy_url);
                
                if !dry_run {
                    println!("🔒 Proxy: listening on :{} ({} domains allowed)", p.port, final_domains.len());
                }
                Some(p)
            }
            Err(e) => {
                eprintln!("⚠️  Proxy failed to start: {} — continuing without proxy", e);
                None
            }
        }
    } else {
        None
    };

    bwrap
        .arg("--chdir")
        .arg(&project_dir)
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
