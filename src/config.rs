//! config.rs
//!
//! Data structures and I/O for the three-layer config system:
//!   - core.toml (embedded in binary, deserialized into CoreConfig)
//!   - system.toml (scanner output, read/written as SystemConfig)
//!   - cordon.toml (user-defined mounts, read/written as UserConfig)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
/// Kept as a public utility; integrity_check handles system.toml reads
/// internally, so this function is not currently called in the main flow.
#[allow(dead_code)]
pub fn load_system_config() -> Result<SystemConfig> {
    let path = get_config_dir()?.join("system.toml");
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let config: SystemConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(config)
}

/// Writes system.toml to ~/.config/cordon/ with file locking.
pub fn save_system_config(config: &SystemConfig) -> Result<()> {
    let path = get_config_dir()?.join("system.toml");
    let content = toml::to_string_pretty(config)?;

    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;

    let mut lock = fd_lock::RwLock::new(file);
    let mut write_guard = lock.write()?;
    write_guard.set_len(0)?; // truncate
    use std::io::Write;
    write_guard.write_all(content.as_bytes())?;

    Ok(())
}

/// Walks up from the current directory looking for cordon.toml.
/// Returns `None` if no cordon.toml is found before reaching `/`.
///
/// We stop at `/` explicitly rather than relying on `pop()` returning false
/// so that we never accidentally pick up a stray cordon.toml at the root.
pub fn find_user_config() -> Result<Option<UserConfig>> {
    let mut current = std::env::current_dir()
        .context("Cannot determine current working directory")?;
    loop {
        let config_path = current.join("cordon.toml");
        if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read {}", config_path.display()))?;
            let config: UserConfig = toml::from_str(&content)
                .with_context(|| format!("Failed to parse {}", config_path.display()))?;
            return Ok(Some(config));
        }
        if current == std::path::Path::new("/") || !current.pop() {
            break;
        }
    }
    Ok(None)
}

/// Appends a new mount to cordon.toml in the current working directory.
/// Creates cordon.toml if it does not yet exist.
///
/// The mount is exposed at the same path inside the sandbox as it sits on the
/// host ─ i.e. `src` and `dest` are identical. This keeps cordon.toml
/// entries predictable and avoids accidental path aliasing.
pub fn add_user_mount(path: String, mode: String) -> Result<()> {
    let mut config = find_user_config()?.unwrap_or_default();

    // Use the canonical absolute path as both src and dest so the mount
    // appears at the same location inside the sandbox.
    let abs_path = std::fs::canonicalize(&path)
        .unwrap_or_else(|_| std::path::PathBuf::from(&path));
    let abs_str = abs_path.to_string_lossy().to_string();

    config.mounts.push(UserMount {
        src: abs_str.clone(),
        dest: abs_str,
        mode,
        when: "always".to_string(),
        required: false,
    });

    let content = toml::to_string_pretty(&config)
        .context("Failed to serialise cordon.toml")?;
    fs::write("cordon.toml", content)
        .context("Failed to write cordon.toml")?;
    println!("✅ Added mount to cordon.toml");
    Ok(())
}
