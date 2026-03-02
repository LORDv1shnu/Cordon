use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use std::fs;
use std::path::{PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CoreModule {
    pub name: String,
    pub description: String,
    pub default_dir: String,
    pub required_files: Vec<String>,
    pub functionality: String,
    pub mode: String,
    pub when: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CoreConfig {
    #[serde(rename = "module")]
    pub modules: Vec<CoreModule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MountEntry {
    pub name: String,
    pub src: String,
    pub dest: String,
    pub bind_type: String, // "ro-bind" or "symlink"
    pub mode: String,
    pub when: String,
    #[serde(default = "default_verified")]
    pub verified: bool,
}

fn default_verified() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemConfig {
    pub last_scan: String,
    pub cordon_version: String,
    #[serde(rename = "mount")]
    pub mounts: Vec<MountEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct UserConfig {
    #[serde(rename = "mount", default)]
    pub mounts: Vec<UserMount>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserMount {
    pub src: String,
    pub dest: String,
    pub mode: String,
    pub when: String,
}

pub fn get_config_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let path = PathBuf::from(home).join(".config").join("cordon");
    if !path.exists() {
        fs::create_dir_all(&path)?;
    }
    Ok(path)
}

pub fn load_system_config() -> Result<SystemConfig> {
    let path = get_config_dir()?.join("system.toml");
    let content = fs::read_to_string(path)?;
    let config: SystemConfig = toml::from_str(&content)?;
    Ok(config)
}

pub fn save_system_config(config: &SystemConfig) -> Result<()> {
    let path = get_config_dir()?.join("system.toml");
    let content = toml::to_string_pretty(config)?;
    fs::write(path, content)?;
    Ok(())
}

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

pub fn add_user_mount(path: String, mode: String) -> Result<()> {
    let mut config = find_user_config()?.unwrap_or_default();
    let dest = format!("/project/{}", path.trim_start_matches('/'));
    
    config.mounts.push(UserMount {
        src: path,
        dest,
        mode,
        when: "always".to_string(),
    });
    let content = toml::to_string_pretty(&config)?;
    fs::write("cordon.toml", content)?;
    println!("✅ Added mount to cordon.toml");
    Ok(())
}
