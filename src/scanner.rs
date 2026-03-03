//! scanner.rs
//!
//! Responsible for detecting host system layout and generating
//! ~/.config/cordon/system.toml.
//!
//! This module:
//! - Reads embedded core.toml
//! - Scans filesystem layout
//! - Detects merged-usr symlinks
//! - Verifies required files
//! - Produces SystemConfig
//!
//! This module does NOT execute sandbox logic.

use anyhow::{Result, Context};
use crate::config::{CoreConfig, CoreModule, SystemConfig, MountEntry};
use std::fs;
use std::path::Path;
use chrono::Utc;

const CORE_TOML: &str = include_str!("../config/core.toml");

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
    let path = Path::new(&module.default_dir);

    if !path.exists() {
        println!(
            "⚠️ Warning: Module {} not found at {}",
            module.name,
            module.default_dir
        );

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
                println!(
                    "⚠️ Required file missing for module {}: {}",
                    module.name,
                    resolved_target.join(file).display()
                );
                break;
            }
        }

        return Ok(Some(MountEntry {
            name: module.name.clone(),
            src: raw_target.to_string_lossy().to_string(), // IMPORTANT: keep raw link
            dest: module.default_dir.clone(),
            bind_type: "symlink".to_string(),
            mode: module.mode.clone(),
            when: module.when.clone(),
            verified,
        }));
    }

    // --- NORMAL DIRECTORY HANDLING ---
    let mut verified = true;

    for file in &module.required_files {
        if !path.join(file).exists() {
            verified = false;
            println!(
                "⚠️ Required file missing for module {}: {}",
                module.name,
                path.join(file).display()
            );
            break;
        }
    }

    Ok(Some(MountEntry {
        name: module.name.clone(),
        src: module.default_dir.clone(),
        dest: module.default_dir.clone(),
        bind_type: "ro-bind".to_string(),
        mode: module.mode.clone(),
        when: module.when.clone(),
        verified,
    }))
}

/// Ensures system.toml exists and is valid before sandbox execution.
pub fn pre_flight_check() -> Result<SystemConfig> {
    let system_path =
        crate::config::get_config_dir()?.join("system.toml");

    if !system_path.exists() {
        run_scan()?;
    }

    let config =
        crate::config::load_system_config()
        .context("Failed to load system.toml")?;

    Ok(config)
}