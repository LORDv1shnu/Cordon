use anyhow::{Result, Context};
use crate::config::{CoreConfig, CoreModule, SystemConfig, MountEntry};
use std::fs;
use std::path::{Path, PathBuf};
use chrono::Utc;

const CORE_TOML: &str = include_str!("../config/core.toml");

pub fn run_scan() -> Result<()> {
    println!("🔍 Scanning system for configuration...");
    let core: CoreConfig = toml::from_str(CORE_TOML).context("Failed to parse embedded core.toml")?;
    let mut mounts = Vec::new();

    for module in &core.modules {
        if let Some(mount) = scan_module(module)? {
            mounts.push(mount);
        }
    }

    let system_config = SystemConfig {
        last_scan: Utc::now().to_rfc3339(),
        cordon_version: env!("CARGO_PKG_VERSION").to_string(),
        mounts: mounts,
    };

    crate::config::save_system_config(&system_config)?;
    println!("✅ system.toml generated successfully");
    Ok(())
}

fn check_foreign_entries(config: &mut SystemConfig, core: &CoreConfig) -> Result<bool> {
    let mut modified = false;
    let mut to_remove = Vec::new();

    for (i, mount) in config.mounts.iter().enumerate() {
        if !core.modules.iter().any(|m| m.name == mount.name) {
            println!("🚨 FOREIGN ENTRY detected: {} -> {}", mount.name, mount.src);
            println!("This entry does not belong to core modules.");
            println!("[D] Discard - remove from system.toml");
            println!("[M] Move    - move to user.toml instead");
            
            // For now, default to Discard in non-interactive environment
            // In a real TUI we would prompt here.
            to_remove.push(i);
            modified = true;
        }
    }

    for i in to_remove.iter().rev() {
        config.mounts.remove(*i);
    }

    Ok(modified)
}

fn scan_module(module: &CoreModule) -> Result<Option<MountEntry>> {
    let path = Path::new(&module.default_dir);
    if !path.exists() {
        println!("⚠️ Warning: Module {} not found at default path {}", module.name, module.default_dir);
        // For network, we mark verified=false if it's missing
        return Ok(Some(MountEntry {
            name: module.name.clone(),
            src: module.default_dir.clone(),
            dest: module.default_dir.clone(),
            bind_type: "ro-bind".to_string(),
            mode: module.mode.clone(),
            when: module.when.clone(),
            verified: false,
        }));
    }

    // Symlink detection
    let metadata = fs::symlink_metadata(path)?;
    let (bind_type, src) = if metadata.is_symlink() {
        match fs::read_link(path) {
            Ok(target) => ("symlink".to_string(), target.to_str().unwrap_or("").to_string()),
            Err(_) => ("ro-bind".to_string(), module.default_dir.clone()),
        }
    } else {
        ("ro-bind".to_string(), module.default_dir.clone())
    };

    // Verify required files
    let mut verified = true;
    for file in &module.required_files {
        if !path.join(file).exists() {
            verified = false;
            break;
        }
    }

    Ok(Some(MountEntry {
        name: module.name.clone(),
        src: src,
        dest: module.default_dir.clone(),
        bind_type: bind_type,
        mode: module.mode.clone(),
        when: module.when.clone(),
        verified: verified,
    }))
}

pub fn pre_flight_check() -> Result<SystemConfig> {
    let system_path = crate::config::get_config_dir()?.join("system.toml");
    if !system_path.exists() {
        run_scan()?;
    }
    
    let mut config = crate::config::load_system_config().context("Failed to load system.toml")?;
    let core: CoreConfig = toml::from_str(CORE_TOML).context("Failed to parse embedded core.toml")?;

    // Version check
    if config.cordon_version != env!("CARGO_PKG_VERSION") {
        println!("🔄 Cordon version mismatch, re-scanning...");
        run_scan()?;
        return crate::config::load_system_config().context("Failed to reload system.toml");
    }

    // Foreign entry check
    if check_foreign_entries(&mut config, &core)? {
        crate::config::save_system_config(&config)?;
    }

    // Integrity check
    for mount in &config.mounts {
        let path = Path::new(&mount.dest);
        if !path.exists() {
            println!("❌ Integrity failure: {} missing at {}", mount.name, mount.dest);
            run_scan()?;
            return crate::config::load_system_config().context("Failed to reload system.toml");
        }
    }
    
    Ok(config)
}
