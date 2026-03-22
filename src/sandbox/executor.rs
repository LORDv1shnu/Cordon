use crate::sandbox::builder::{apply_environment, build_bwrap};
use crate::sandbox::mounts::{apply_system_mounts, apply_user_mounts};
use crate::scanner::integrity_check;
use crate::errors::CordonError;
use anyhow::Result;
use std::env;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

use crate::sandbox::network::NetworkMode;
use crate::sandbox::proxy::ProxyHandle;

#[derive(Debug, Clone)]
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
    pub seccomp: Option<crate::sandbox::seccomp::SeccompPreset>,
}

pub fn run_sandboxed(opts: SandboxOptions) -> Result<()> {
    // ── Check Prerequisites ──────────────────────────────────────────────────
    if std::process::Command::new("bwrap")
        .arg("--version")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        error!("bubblewrap (bwrap) is not installed or not found in PATH");
        return Err(CordonError::DependencyMissing(
            "bubblewrap (bwrap) — install with: sudo apt install bubblewrap".to_string(),
        ).into());
    }

    // ── Resolve Effective Configuration ──────────────────────────────────────
    let (net, gui, optional, seccomp, _resolved_paths) = resolve_effective_flags(opts.clone())?;

    let SandboxOptions {
        cmd,
        domains,
        dry_run,
        trace,
        quiet,
        verbose,
        mem,
        cpu,
        pid_limit,
        timeout,
        ..
    } = opts;

    if !dry_run && Path::new("cordon.lock").exists() {
        if let Err(e) = crate::commands::lock::run_lock_verify() {
            warn!("Lockfile verification failed: {}. Your system may have changed since the lock was created.", e);
        }
    }

    if !quiet {
        info!("Running inside sandbox...");
    }

    let project_dir: PathBuf = env::current_dir()?;
    let project_path = project_dir.to_str().unwrap();
    let src_dir = project_dir.join("src");
    let has_src = src_dir.exists() && src_dir.is_dir();

    let mut bwrap = build_bwrap(project_path, net, dry_run);

    // ── Apply Resolved Mounts ───────────────────────────────────────────────
    let system_config = if dry_run && !Path::new("~/.config/cordon/system.toml").exists() {
        crate::config::SystemConfig {
            last_scan: "".to_string(),
            cordon_version: env!("CARGO_PKG_VERSION").to_string(),
            mounts: vec![],
        }
    } else {
        integrity_check(net != NetworkMode::Disable, gui)?
    };

    apply_system_mounts(&mut bwrap, &system_config, net != NetworkMode::Disable, gui, &optional);
    apply_user_mounts(&mut bwrap, dry_run);

    if has_src {
        let src_path = src_dir.to_str().unwrap();
        bwrap.args(["--ro-bind", src_path, src_path]);
    }

    apply_environment(&mut bwrap, gui);

    // ── Seccomp Setup ──────────────────────────────────────────────────────────
    let _seccomp_file = if let Some(preset) = seccomp {
        if preset != crate::sandbox::seccomp::SeccompPreset::None {
            match crate::sandbox::seccomp::build_filter(preset) {
                Ok(blob) => {
                    match crate::sandbox::seccomp::write_filter_to_temp_file(&blob) {
                        Ok(file) => {
                            use std::os::unix::io::AsRawFd;
                            bwrap.arg("--seccomp").arg(file.as_raw_fd().to_string());
                            Some(file)
                        }
                        Err(e) => {
                            warn!("Seccomp filter creation failed: {} — continuing without seccomp", e);
                            None
                        }
                    }
                }
                Err(e) => {
                    warn!("Seccomp filter compilation failed: {} — continuing without seccomp", e);
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    // ── Proxy Setup ───────────────────────────────────────────────────────────
    let _proxy = if net == NetworkMode::Allow {
        let proxy_cfg = crate::sandbox::proxy::load_config(&project_dir);
        let mut final_domains = domains.clone();
        final_domains.extend(proxy_cfg.domains);
        final_domains.sort();
        final_domains.dedup();

        match ProxyHandle::spawn(final_domains.clone()) {
            Ok(p) => {
                let proxy_url = format!("http://127.0.0.1:{}", p.port);
                for var in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "ALL_PROXY", "all_proxy", "npm_config_proxy", "npm_config_https_proxy", "PIP_PROXY"] {
                    bwrap.arg("--setenv").arg(var).arg(&proxy_url);
                }
                if !dry_run && !quiet {
                    info!("Proxy listening on :{} ({} domains allowed)", p.port, final_domains.len());
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

    bwrap.arg("--chdir").arg(&project_dir).arg("--").args(&cmd);

    let mut final_command = if trace {
        let strace_log = match crate::config::get_config_dir() {
            Ok(d) => d.join("logs").join("access-denied.log"),
            Err(_) => PathBuf::from("/tmp/access-denied.log"),
        };
        crate::sandbox::tracer::wrap_with_strace(bwrap, &strace_log)
    } else {
        bwrap
    };

    if mem.is_some() || cpu.is_some() || pid_limit.is_some() || timeout.is_some() {
        final_command = crate::sandbox::limits::wrap_with_resource_limits(final_command, mem, cpu, pid_limit, timeout);
    }

    if verbose && !quiet {
        eprintln!("[wrapper] {}", final_command.get_program().to_string_lossy());
        for arg in final_command.get_args() {
            eprintln!("[wrapper] {}", arg.to_string_lossy());
        }
    }

    if dry_run {
        let program = final_command.get_program().to_string_lossy();
        let args: Vec<String> = final_command.get_args().map(|a| a.to_string_lossy().to_string()).collect();
        if !quiet {
            println!("🧪 Dry run mode: command not executed");
        }
        println!("{} {}", program, args.join(" "));
        return Ok(());
    }

    let status = final_command.status()?;
    if trace {
        let trace_log = crate::config::get_config_dir()?.join("logs").join("access-denied.log");
        if let Ok(denied) = crate::sandbox::tracer::parse_strace_log(&trace_log) {
            crate::sandbox::tracer::print_trace_report(&denied, &trace_log);
        }
    }

    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(1);
        error!("Sandboxed command exited with code {}", code);
        let program = cmd.first().cloned().unwrap_or_default();
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

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

pub fn resolve_effective_flags(opts: SandboxOptions) -> Result<(NetworkMode, bool, Vec<String>, Option<crate::sandbox::seccomp::SeccompPreset>, Vec<PathBuf>)> {
    let mut net = opts.net;
    let mut gui = opts.gui;
    let mut optional = opts.optional;
    let mut seccomp = opts.seccomp;
    let mut paths = Vec::new();

    // 1. Profile Layer
    if let Some(ref profile_name) = opts.profile {
        let named = resolve_profile(profile_name)?;
        if !opts.net_is_explicit && let Some(n) = named.network {
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
        if seccomp.is_none() && let Some(s) = named.seccomp {
            seccomp = match s.as_str() {
                "basic" => Some(crate::sandbox::seccomp::SeccompPreset::Basic),
                "strict" => Some(crate::sandbox::seccomp::SeccompPreset::Strict),
                "none" => Some(crate::sandbox::seccomp::SeccompPreset::None),
                _ => None,
            };
        }
    }

    // 2. Project Layer (cordon.toml)
    if let Ok(Some(cfg)) = crate::config::find_user_config() {
        if !opts.net_is_explicit && let Some(n) = cfg.network {
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
        if seccomp.is_none() && let Some(s) = cfg.seccomp {
            seccomp = match s.as_str() {
                "basic" => Some(crate::sandbox::seccomp::SeccompPreset::Basic),
                "strict" => Some(crate::sandbox::seccomp::SeccompPreset::Strict),
                "none" => Some(crate::sandbox::seccomp::SeccompPreset::None),
                _ => None,
            };
        }

        for m in cfg.mounts {
            paths.push(PathBuf::from(m.src));
        }
    }

    // 3. System Layer (system.toml via integrity_check)
    // We only collect paths if we're not in a "pure" resolution (like lock update might want)
    // Actually, lock update DOES want system paths.
    let system_config = crate::scanner::integrity_check(net != NetworkMode::Disable, gui).unwrap_or(crate::config::SystemConfig {
        last_scan: "".to_string(),
        cordon_version: "".to_string(),
        mounts: vec![],
    });

    for m in system_config.mounts {
        let active = match m.when.as_str() {
            "always" => true,
            "network" => net != NetworkMode::Disable,
            "gui" => gui,
            "optional" => optional.contains(&m.name),
            _ => false,
        };
        if active && m.verified {
            paths.push(PathBuf::from(m.src));
        }
    }

    // 4. Implicit paths
    let project_dir = std::env::current_dir()?;
    paths.push(project_dir.clone());
    let src_dir = project_dir.join("src");
    if src_dir.exists() {
        paths.push(src_dir);
    }
    
    if net != NetworkMode::Disable {
        for p in &["/etc/resolv.conf", "/etc/hosts", "/etc/hostname", "/etc/nsswitch.conf", "/etc/ssl", "/etc/pki"] {
            let path = Path::new(p);
            if path.exists() {
                paths.push(path.to_path_buf());
            }
        }
    }

    paths.sort();
    paths.dedup();

    Ok((net, gui, optional, seccomp, paths))
}

fn find_binary(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let path = PathBuf::from(name);
        return if path.exists() { Some(path) } else { None };
    }
    if let Ok(path_var) = env::var("PATH") {
        for dir in path_var.split(':') {
            let full = PathBuf::from(dir).join(name);
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
    let built_in = match name {
        "python" => Some(crate::config::NamedProfile {
            name: "python".to_string(),
            network: Some("allow".to_string()),
            gui: None,
            optional: Some(vec!["ld_so_cache".to_string(), "locale_files".to_string()]),
            seccomp: None,
        }),
        "node" => Some(crate::config::NamedProfile {
            name: "node".to_string(),
            network: Some("allow".to_string()),
            gui: None,
            optional: Some(vec!["ld_so_cache".to_string(), "home_config".to_string()]),
            seccomp: None,
        }),
        "rust" => Some(crate::config::NamedProfile {
            name: "rust".to_string(),
            network: Some("allow".to_string()),
            gui: None,
            optional: Some(vec!["ld_so_cache".to_string()]),
            seccomp: None,
        }),
        "gui-app" => Some(crate::config::NamedProfile {
            name: "gui-app".to_string(),
            network: None,
            gui: Some(true),
            optional: Some(vec!["audio_pipewire".to_string(), "dbus_session".to_string(), "gpu_dri".to_string()]),
            seccomp: None,
        }),
        _ => None,
    };
    if let Some(p) = built_in {
        Ok(p)
    } else {
        anyhow::bail!("Profile '{}' not found.", name);
    }
}
