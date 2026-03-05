//! scanner.rs
//!
//! Responsible for detecting host system layout and generating
//! ~/.config/cordon/system.toml.

use anyhow::{Result, Context, bail};
use crate::config::{CoreConfig, CoreModule, SystemConfig, MountEntry};
use std::fs;
use std::path::{Path, PathBuf};
use chrono::Utc;

const CORE_TOML: &str = include_str!("../config/core.toml");

fn resolve_env_vars(path: &str) -> String {
    if path.starts_with("/run/user/1000") {
        if let Ok(val) = std::env::var("XDG_RUNTIME_DIR") {
            return path.replace("/run/user/1000", &val);
        }
    }
    path.to_string()
}

/// Runs a full system scan and regenerates system.toml.
pub fn run_scan() -> Result<()> {
    println!("🔍 Scanning system for configuration...");

    let core: CoreConfig =
        toml::from_str(CORE_TOML)
        .context("Failed to parse embedded core.toml")?;

    let mut mounts = Vec::new();

    for module in &core.modules {
        if let Some(mount) = scan_module(module)? {
            mounts.push(mount);
        }
    }

    let system_config = SystemConfig {
        last_scan: Utc::now().to_rfc3339(),
        cordon_version: env!("CARGO_PKG_VERSION").to_string(),
        mounts,
    };

    crate::config::save_system_config(&system_config)?;
    println!("✅ system.toml generated successfully");

    Ok(())
}

fn scan_module(module: &CoreModule) -> Result<Option<MountEntry>> {
    let resolved_dir = resolve_env_vars(&module.default_dir);
    let path = Path::new(&resolved_dir);
    
    let dest = if module.default_dir.contains("/run/user/1000") {
        resolved_dir.clone()
    } else {
        module.default_dir.clone()
    };

    if !path.exists() {
        if module.required {
            println!(
                "⚠️ Warning: Required module {} not found at {}",
                module.name,
                resolved_dir
            );
        }

        return Ok(Some(MountEntry {
            name: module.name.clone(),
            src: resolved_dir.clone(),
            dest,
            bind_type: if module.mode == "rw" { "bind".to_string() } else { "ro-bind".to_string() },
            mode: module.mode.clone(),
            when: module.when.clone(),
            required: module.required,
            verified: false,
        }));
    }

    let metadata = fs::symlink_metadata(path)?;

    // --- SYMLINK HANDLING ---
    if metadata.file_type().is_symlink() {
        let raw_target = fs::read_link(path)?; // original link content

        // Resolve for verification only
        let resolved_target = if raw_target.is_absolute() {
            raw_target.clone()
        } else {
            path.parent()
                .unwrap_or(Path::new("/"))
                .join(&raw_target)
        };

        // Verify required files using resolved path
        let mut verified = true;

        for file in &module.required_files {
            if !resolved_target.join(file).exists() {
                verified = false;
                if module.required {
                    println!(
                        "⚠️ Required file missing for module {}: {}",
                        module.name,
                        resolved_target.join(file).display()
                    );
                }
                break;
            }
        }

        return Ok(Some(MountEntry {
            name: module.name.clone(),
            src: raw_target.to_string_lossy().to_string(), // IMPORTANT: keep raw link
            dest,
            bind_type: "symlink".to_string(),
            mode: module.mode.clone(),
            when: module.when.clone(),
            required: module.required,
            verified,
        }));
    }

    // --- NORMAL DIRECTORY HANDLING ---
    let mut verified = true;

    for file in &module.required_files {
        if !path.join(file).exists() {
            verified = false;
            if module.required {
                println!(
                    "⚠️ Required file missing for module {}: {}",
                    module.name,
                    path.join(file).display()
                );
            }
            break;
        }
    }

    Ok(Some(MountEntry {
        name: module.name.clone(),
        src: resolved_dir.clone(),
        dest,
        bind_type: if module.mode == "rw" { "bind".to_string() } else { "ro-bind".to_string() },
        mode: module.mode.clone(),
        when: module.when.clone(),
        required: module.required,
        verified,
    }))
}

/// Ensures system.toml exists and is valid before sandbox execution.
pub fn pre_flight_check(network: bool, gui: bool) -> Result<SystemConfig> {
    let system_path =
        crate::config::get_config_dir()?.join("system.toml");

    if !system_path.exists() {
        println!("📝 system.toml not found. Triggering initial scan...");
        run_scan()?;
    }

    let content = match fs::read_to_string(&system_path) {
        Ok(c) => c,
        Err(_) => {
            println!("⚠️ Failed to read system.toml. Triggering scan...");
            run_scan()?;
            fs::read_to_string(&system_path)?
        }
    };

    let mut config: SystemConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(_) => {
            println!("⚠️ system.toml is malformed. Triggering scan...");
            run_scan()?;
            let new_content = fs::read_to_string(&system_path)?;
            toml::from_str(&new_content)?
        }
    };

    let current_version = env!("CARGO_PKG_VERSION");
    if config.cordon_version != current_version {
        println!("🔄 Cordon version updated ({} -> {}). Triggering scan...", config.cordon_version, current_version);
        run_scan()?;
        let new_content = fs::read_to_string(&system_path)?;
        config = toml::from_str(&new_content)?;
    }

    let core: CoreConfig = toml::from_str(CORE_TOML).context("Failed to parse embedded core.toml")?;
    
    // Quick integrity check + Foreign entry detection
    let mut needs_rescan_modules = Vec::new();
    for mount in &config.mounts {
        let core_module = core.modules.iter().find(|m| m.name == mount.name);
        if core_module.is_none() {
            println!("❌ Foreign entry detected in system.toml: {}", mount.name);
            println!("Foreign entries are a security risk. Please manually remove it or move it to user.toml.");
            bail!("Foreign entry detected in system.toml: {}", mount.name);
        }

        let core_module = core_module.unwrap();
        // File-first integrity check
        let path = if mount.bind_type == "symlink" {
            // Check resolved target
            let link_path = Path::new(&mount.dest);
            if !link_path.exists() || !fs::symlink_metadata(link_path).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
                needs_rescan_modules.push(mount.name.clone());
                continue;
            }
            let raw_target = PathBuf::from(&mount.src);
            if raw_target.is_absolute() { raw_target } else { link_path.parent().unwrap_or(Path::new("/")).join(&raw_target) }
        } else {
            PathBuf::from(&mount.src)
        };

        if mount.verified {
            for file in &core_module.required_files {
                if !path.join(file).exists() {
                    needs_rescan_modules.push(mount.name.clone());
                    break;
                }
            }
        }
    }

    if !needs_rescan_modules.is_empty() {
        println!("⚠️ System integrity check failed. Triggering partial scan...");
        for module_name in needs_rescan_modules {
            if let Some(core_module) = core.modules.iter().find(|m| m.name == module_name) {
                if let Some(new_mount) = scan_module(core_module)? {
                    if let Some(existing) = config.mounts.iter_mut().find(|m| m.name == module_name) {
                        *existing = new_mount;
                    }
                }
            }
        }
        crate::config::save_system_config(&config)?;
    }

    if network {
        for mount in &config.mounts {
            if mount.when == "network" && mount.required && !mount.verified {
                bail!("Missing required network module: {}. Network mode cannot run safely.", mount.name);
            }
        }
    }

    if gui {
        for mount in &config.mounts {
            if mount.when == "gui" && mount.required && !mount.verified {
                bail!("Missing required GUI module: {}. GUI mode cannot run safely.", mount.name);
            }
        }
    }

    Ok(config)
}
