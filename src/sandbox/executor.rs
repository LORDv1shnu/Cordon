use crate::sandbox::builder::{apply_environment, build_bwrap};
use crate::sandbox::mounts::{apply_system_mounts, apply_user_mounts};
use crate::scanner::integrity_check;
use crate::errors::CordonError;
use anyhow::Result;
use std::env;
use std::path::PathBuf;
use tracing::{error, info, warn};

use crate::sandbox::network::NetworkMode;
use crate::sandbox::proxy::ProxyHandle;

/// Builds and executes the bubblewrap sandbox.
///
/// Flow:
///   1. Check bwrap is installed
///   2. Run integrity_check() to validate system.toml
///   3. Build bwrap command: namespace isolation, system mounts, user mounts
///   4. Prompt user before applying cordon.toml mounts
///   5. Forward safe environment variables into sandbox
///   6. Execute command (or print in dry-run mode)
///
/// All mount paths come from system.toml and cordon.toml — nothing is hardcoded.
pub fn run_sandboxed(
    cmd: Vec<String>,
    mut net: NetworkMode,
    domains: Vec<String>,
    dry_run: bool,
    mut gui: bool,
    mut optional: Vec<String>,
) -> Result<()> {
    // 0. Merge user profile defaults if CLI arguments are at default values
    if let Ok(Some(cfg)) = crate::config::find_user_config() {
        if net == NetworkMode::Disable {
            if let Some(n) = cfg.network {
                net = match n.as_str() {
                    "allow" => NetworkMode::Allow,
                    "full" => NetworkMode::Full,
                    _ => NetworkMode::Disable,
                };
            }
        }
        if !gui {
            gui = cfg.gui.unwrap_or(false);
        }
        if let Some(opts) = cfg.optional {
            for opt in opts {
                if !optional.contains(&opt) {
                    optional.push(opt);
                }
            }
        }
    }

    // 1. Verify bwrap is installed.
    if std::process::Command::new("bwrap")
        .arg("--version")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        error!("bubblewrap (bwrap) is not installed or not found in PATH");
        return Err(CordonError::DependencyMissing(
            "bubblewrap (bwrap) — install with: sudo apt install bubblewrap".to_string(),
        )
        .into());
    }

    info!("Running inside sandbox...");

    let project_dir: PathBuf = env::current_dir()?;
    let project_path = project_dir.to_str().unwrap();
    let src_dir = project_dir.join("src");
    let has_src = src_dir.exists() && src_dir.is_dir();

    if has_src && !dry_run {
        info!("Protecting src/ as read-only");
    }

    if !dry_run {
        info!("Project dir: {}", project_dir.display());
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

    // ── Proxy Setup ───────────────────────────────────────────────────────────
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
                // npm and pip have their own proxy vars that ignore the standard ones
                bwrap.arg("--setenv").arg("npm_config_proxy").arg(&proxy_url);
                bwrap.arg("--setenv").arg("npm_config_https_proxy").arg(&proxy_url);
                bwrap.arg("--setenv").arg("PIP_PROXY").arg(&proxy_url);

                if !dry_run {
                    info!(
                        "Proxy listening on :{} ({} domains allowed)",
                        p.port,
                        final_domains.len()
                    );
                }
                Some(p)
            }
            Err(e) => {
                warn!("Proxy failed to start: {} — continuing without proxy", e);
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
        Ok(())
    } else {
        let code = status.code().unwrap_or(1);
        error!("Sandboxed command exited with code {}", code);
        // Determine the most specific error type possible
        let program = cmd.first().cloned().unwrap_or_default();
        // Exit codes 1/126/127 may mean the binary wasn't found or isn't executable.
        if matches!(code, 1 | 126 | 127) {
            if let Some(path) = find_binary(&program) {
                if !is_executable(&path) {
                    return Err(CordonError::PermissionDenied(program).into());
                }
            } else {
                return Err(CordonError::CommandNotFound(program).into());
            }
        }
        Err(CordonError::ExecutionError(code).into())
    }
}

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn find_binary(name: &str) -> Option<std::path::PathBuf> {
    if name.contains('/') {
        let path = std::path::PathBuf::from(name);
        return if path.exists() { Some(path) } else { None };
    }
    if let Ok(path_var) = env::var("PATH") {
        for dir in path_var.split(':') {
            let full = std::path::PathBuf::from(dir).join(name);
            if full.exists() {
                return Some(full);
            }
        }
    }
    None
}
