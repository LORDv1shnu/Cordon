//! config.rs
//!
//! Data structures and I/O for the three-layer config system:
//!   - core.toml (embedded in binary, deserialized into CoreConfig)
//!   - system.toml (scanner output, read/written as SystemConfig)
//!   - cordon.toml (user-defined mounts, read/written as UserConfig)

use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use std::fs;
use std::path::{PathBuf};

/// A single module entry from core.toml (the embedded blueprint).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CoreModule {
    pub name: String,
    pub description: String,
    pub default_dir: String,
    pub required_files: Vec<String>,
    pub functionality: String,
    pub mode: String,
    pub when: String,
    pub required: bool,
}

/// Top-level structure of core.toml. Contains all module definitions.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CoreConfig {
    #[serde(rename = "module")]
    pub modules: Vec<CoreModule>,
}

/// A single verified mount entry written to system.toml by the scanner.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MountEntry {
    pub name: String,
    pub src: String,
    pub dest: String,
    pub bind_type: String, // "ro-bind" or "symlink"
    pub mode: String,
    pub when: String,
    pub required: bool,
    #[serde(default = "default_verified")]
    pub verified: bool,
}

fn default_verified() -> bool {
    true
}

/// Top-level structure of system.toml (scanner output).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemConfig {
    pub last_scan: String,
    pub cordon_version: String,
    #[serde(rename = "mount")]
    pub mounts: Vec<MountEntry>,
}

/// Top-level structure of cordon.toml (user-defined mounts).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct UserConfig {
    #[serde(rename = "mount", default)]
    pub mounts: Vec<UserMount>,
}

/// A single user-defined mount entry from cordon.toml.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserMount {
    pub src: String,
    pub dest: String,
    pub mode: String,
    pub when: String,
    pub required: bool,
}

/// Returns the path to ~/.config/cordon/, creating it if needed.
pub fn get_config_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let path = PathBuf::from(home).join(".config").join("cordon");
    if !path.exists() {
        fs::create_dir_all(&path)?;
    }
    Ok(path)
}

/// Reads and parses system.toml from ~/.config/cordon/.
pub fn load_system_config() -> Result<SystemConfig> {
    let path = get_config_dir()?.join("system.toml");
    let content = fs::read_to_string(path)?;
    let config: SystemConfig = toml::from_str(&content)?;
    Ok(config)
}

/// Writes system.toml to ~/.config/cordon/ with file locking.
pub fn save_system_config(config: &SystemConfig) -> Result<()> {
    let path = get_config_dir()?.join("system.toml");
    let content = toml::to_string_pretty(config)?;
    
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&path)?;
    
    let mut lock = fd_lock::RwLock::new(file);
    let mut write_guard = lock.write()?;
    write_guard.set_len(0)?; // truncate
    use std::io::Write;
    write_guard.write_all(content.as_bytes())?;
    
    Ok(())
}

/// Walks up from the current directory looking for cordon.toml.
/// Returns None if no cordon.toml is found before reaching /.
pub fn find_user_config() -> Result<Option<UserConfig>> {
    let mut current = std::env::current_dir()?;
    loop {
        let config_path = current.join("cordon.toml");
        if config_path.exists() {
            let content = fs::read_to_string(config_path)?;
            let config: UserConfig = toml::from_str(&content)?;
            return Ok(Some(config));
        }
        if !current.pop() {
            break;
        }
        // Don't go outside home directory if possible
        if current == PathBuf::from("/") {
            break;
        }
    }
    Ok(None)
}

/// Appends a new mount to cordon.toml in the current directory.
/// Creates the file if it doesn't exist.
pub fn add_user_mount(path: String, mode: String) -> Result<()> {
    let mut config = find_user_config()?.unwrap_or_default();
    let dest = format!("/project/{}", path.trim_start_matches('/'));
    
    config.mounts.push(UserMount {
        src: path,
        dest,
        mode,
        when: "always".to_string(),
        required: true,
    });
    let content = toml::to_string_pretty(&config)?;
    fs::write("cordon.toml", content)?;
    println!("✅ Added mount to cordon.toml");
    Ok(())
}
