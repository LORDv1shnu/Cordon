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
pub struct SandboxOptions {
    pub cmd: Vec<String>,
    pub net: NetworkMode,
    pub domains: Vec<String>,
    pub dry_run: bool,
    pub gui: bool,
    pub optional: Vec<String>,
    pub profile: Option<String>,
    pub trace: bool,
    pub quiet: bool,
    pub verbose: bool,
    pub net_is_explicit: bool,
    pub mem: Option<String>,
    pub cpu: Option<f32>,
    pub pid_limit: Option<u32>,
    pub timeout: Option<u64>,
}

pub fn run_sandboxed(opts: SandboxOptions) -> Result<()> {
    let mut net = opts.net;
    let mut gui = opts.gui;
    let mut optional = opts.optional;

    let SandboxOptions {
        cmd,
        domains,
        dry_run,
        // gui, // Handled above as mutable
        // optional, // Handled above as mutable
        profile,
        trace,
        quiet,
        verbose,
        net_is_explicit,
        mem,
        cpu,
        pid_limit,
        timeout,
        .. // Ignore other fields already extracted
    } = opts;

    // 0a. Resolve named profile (before cordon.toml merge)
    if let Some(ref profile_name) = profile {
        let named = resolve_profile(profile_name)?;
        if !net_is_explicit
            && let Some(n) = named.network {
                net = match n.as_str() {
                    "allow" => NetworkMode::Allow,
                    "full" => NetworkMode::Full,
                    _ => NetworkMode::Disable,
                };
            }
        if !gui {
            gui = named.gui.unwrap_or(false);
        }
        if let Some(opts) = named.optional {
            for opt in opts {
                if !optional.contains(&opt) {
                    optional.push(opt);
                }
            }
        }
    }

    // 0b. Merge user profile defaults if CLI arguments are at default values
    if let Ok(Some(cfg)) = crate::config::find_user_config() {
        if !net_is_explicit
            && let Some(n) = cfg.network {
                net = match n.as_str() {
                    "allow" => NetworkMode::Allow,
                    "full" => NetworkMode::Full,
                    _ => NetworkMode::Disable,
                };
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

    if !quiet {
        info!("Running inside sandbox...");
    }

    let project_dir: PathBuf = env::current_dir()?;
    let project_path = project_dir.to_str().unwrap();
    let src_dir = project_dir.join("src");
    let has_src = src_dir.exists() && src_dir.is_dir();

    if has_src && !dry_run && !quiet {
        info!("Protecting src/ as read-only");
    }

    if !dry_run && !quiet {
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

                if !dry_run && !quiet {
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

    let mut final_command = if trace {
        if std::process::Command::new("strace")
            .arg("--version")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            error!("strace is not installed or not found in PATH");
            return Err(CordonError::DependencyMissing(
                "strace — install with: sudo apt install strace".to_string(),
            )
            .into());
        }
        
        let trace_log = crate::config::get_config_dir()?.join("logs").join("last-trace.log");
        if !quiet {
            info!("Tracing denied accesses to {}", trace_log.display());
        }
        crate::sandbox::tracer::wrap_with_strace(bwrap, &trace_log)
    } else {
        bwrap
    };

    let has_limits = mem.is_some() || cpu.is_some() || pid_limit.is_some() || timeout.is_some();
    if has_limits {
        if std::process::Command::new("systemd-run")
            .arg("--version")
            .output()
            .is_err()
        {
            error!("systemd-run is required for resource limits but was not found in PATH");
            return Err(CordonError::DependencyMissing(
                "systemd-run — required for --mem, --cpu, --pid-limit, --timeout".to_string(),
            )
            .into());
        }
        final_command = crate::sandbox::limits::wrap_with_resource_limits(
            final_command,
            mem,
            cpu,
            pid_limit,
            timeout,
        );
    }

    if dry_run {
        let program = final_command.get_program().to_string_lossy();
        let args = final_command
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");

        if !quiet {
            println!("🧪 Dry run mode: command not executed");
        }
        println!("{} {}", program, args);
        return Ok(());
    }

    if verbose && !quiet {
        let program = final_command.get_program().to_string_lossy();
        eprintln!("[wrapper] {}", program);
        for arg in final_command.get_args() {
            eprintln!("[wrapper] {}", arg.to_string_lossy());
        }
    }

    let status = final_command.status()?;
    
    if trace {
        let trace_log = crate::config::get_config_dir()?.join("logs").join("last-trace.log");
        if let Ok(denied) = crate::sandbox::tracer::parse_strace_log(&trace_log) {
            crate::sandbox::tracer::print_trace_report(&denied, &trace_log);
        }
    }

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

fn resolve_profile(name: &str) -> Result<crate::config::NamedProfile> {
    let config = crate::config::load_profiles().unwrap_or_default();
    if let Some(p) = config.profiles.into_iter().find(|p| p.name == name) {
        return Ok(p);
    }
    
    // Check built-in profiles if it's not a saved profile
    let built_in = match name {
        "python" => Some(crate::config::NamedProfile {
            name: "python".to_string(),
            network: Some("allow".to_string()),
            gui: None,
            optional: Some(vec!["ld_so_cache".to_string(), "locale_files".to_string()]),
        }),
        "node" => Some(crate::config::NamedProfile {
            name: "node".to_string(),
            network: Some("allow".to_string()),
            gui: None,
            optional: Some(vec!["ld_so_cache".to_string(), "home_config".to_string()]),
        }),
        "rust" => Some(crate::config::NamedProfile {
            name: "rust".to_string(),
            network: Some("allow".to_string()),
            gui: None,
            optional: Some(vec!["ld_so_cache".to_string()]),
        }),
        "gui-app" => Some(crate::config::NamedProfile {
            name: "gui-app".to_string(),
            network: None,
            gui: Some(true),
            optional: Some(vec![
                "audio_pipewire".to_string(),
                "dbus_session".to_string(),
                "gpu_dri".to_string(),
            ]),
        }),
        _ => None,
    };
    
    if let Some(p) = built_in {
        Ok(p)
    } else {
        anyhow::bail!("Profile '{}' not found. Use 'cordon profile list' to see available profiles.", name);
    }
}

