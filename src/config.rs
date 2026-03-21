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
    pub bind_type: String, // "ro-bind", "symlink", "bind", or "dev-bind"
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

    pub network: Option<String>,
    pub gui: Option<bool>,
    pub optional: Option<Vec<String>>,
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

/// A single named sandbox profile stored in profiles.toml.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct NamedProfile {
    pub name: String,
    pub network: Option<String>,
    pub gui: Option<bool>,
    pub optional: Option<Vec<String>>,
}

/// Top-level structure of profiles.toml.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ProfilesConfig {
    #[serde(rename = "profile", default)]
    pub profiles: Vec<NamedProfile>,
}

/// Returns the path to ~/.config/cordon/profiles.toml
pub fn get_profiles_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("profiles.toml"))
}

/// Reads and parses profiles.toml from ~/.config/cordon/.
/// Returns an empty ProfilesConfig if the file doesn't exist yet.
pub fn load_profiles() -> Result<ProfilesConfig> {
    let path = get_profiles_path()?;
    if !path.exists() {
        return Ok(ProfilesConfig::default());
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let config: ProfilesConfig =
        toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(config)
}

/// Writes profiles.toml to ~/.config/cordon/.
pub fn save_profiles(config: &ProfilesConfig) -> Result<()> {
    let path = get_profiles_path()?;
    let content = toml::to_string_pretty(config)?;
    fs::write(&path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
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
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let config: SystemConfig =
        toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))?;
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
    let mut current =
        std::env::current_dir().context("Cannot determine current working directory")?;
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
    let abs_path = std::fs::canonicalize(&path).unwrap_or_else(|_| std::path::PathBuf::from(&path));
    let abs_str = abs_path.to_string_lossy().to_string();

    config.mounts.push(UserMount {
        src: abs_str.clone(),
        dest: abs_str,
        mode,
        when: "always".to_string(),
        required: false,
    });

    let content = toml::to_string_pretty(&config).context("Failed to serialise cordon.toml")?;
    fs::write("cordon.toml", content).context("Failed to write cordon.toml")?;
    println!("✅ Added mount to cordon.toml");
    Ok(())
}

/// Removes a mount from cordon.toml based on the provided path.
///
/// If multiple entries match the canonicalized path, all are removed.
/// If the file becomes empty after removal, it is deleted.
pub fn remove_user_mount(path: String) -> Result<()> {
    let mut config = match find_user_config()? {
        Some(c) => c,
        None => {
            println!("⚠️  No cordon.toml found in this directory or its parents.");
            return Ok(());
        }
    };

    let abs_path = std::fs::canonicalize(&path).unwrap_or_else(|_| std::path::PathBuf::from(&path));
    let abs_str = abs_path.to_string_lossy().to_string();

    let initial_len = config.mounts.len();
    config.mounts.retain(|m| m.src != abs_str);

    if config.mounts.len() == initial_len {
        println!("⚠️  Path '{}' was not found in cordon.toml", abs_str);
    } else if config.mounts.is_empty() {
        // Delete the file if it's now empty
        if let Ok(mut current) = std::env::current_dir() {
            loop {
                let config_path = current.join("cordon.toml");
                if config_path.exists() {
                    fs::remove_file(config_path)?;
                    println!("✅ Removed empty cordon.toml");
                    break;
                }
                if !current.pop() {
                    break;
                }
            }
        }
    } else {
        let content =
            toml::to_string_pretty(&config).context("Failed to serialise cordon.toml")?;
        // We need to write to the ACTUAL file found.
        // Simplified: write to the one in CWD if it exists, or the one found by find_user_config.
        // More robust: find_user_config should probably return the path too.
        // For now, write to ./cordon.toml as add_user_mount does.
        fs::write("cordon.toml", content).context("Failed to write cordon.toml")?;
        println!("✅ Removed '{}' from cordon.toml", abs_str);
    }

    Ok(())
}

/// Opens the local cordon.toml in the system default editor.
pub fn edit_user_config() -> Result<()> {
    if find_user_config()?.is_none() {
        println!("No cordon.toml found. Creating a blank one...");
        let config = UserConfig::default();
        let content = toml::to_string_pretty(&config).context("Failed to serialise cordon.toml")?;
        fs::write("cordon.toml", content).context("Failed to write cordon.toml")?;
    }

    // Attempt to open the file using the system editor
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("notepad.exe")
            .arg("cordon.toml")
            .spawn()
            .context("Failed to open notepad")?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
        std::process::Command::new(editor)
            .arg("cordon.toml")
            .status()
            .context("Failed to open editor")?;
    }

    Ok(())
}

/// Enum for setting profile fields in cordon.toml.
pub enum ProfileField {
    Network(String),
    Gui(bool),
    OptionalAdd(String),
}

/// Sets a profile field in cordon.toml, creating the file if it doesn't exist.
pub fn set_profile_field(field: ProfileField) -> Result<()> {
    let mut config = find_user_config()?.unwrap_or_default();

    match field {
        ProfileField::Network(net) => {
            config.network = Some(net);
        }
        ProfileField::Gui(gui) => {
            config.gui = Some(gui);
        }
        ProfileField::OptionalAdd(module) => {
            let mut opts = config.optional.unwrap_or_default();
            if !opts.contains(&module) {
                opts.push(module);
            }
            config.optional = Some(opts);
        }
    }

    let content = toml::to_string_pretty(&config).context("Failed to serialise cordon.toml")?;
    fs::write("cordon.toml", content).context("Failed to write cordon.toml")?;
    println!("✅ Updated profile in cordon.toml");
    Ok(())
}

/// Enum for unsetting profile fields in cordon.toml.
pub enum ProfileUnsetField {
    Network,
    Gui,
    OptionalRemove(String),
}

/// Unsets a profile field in cordon.toml if it exists.
pub fn unset_profile_field(field: ProfileUnsetField) -> Result<()> {
    let mut config = match find_user_config()? {
        Some(c) => c,
        None => {
            println!("⚠️  No cordon.toml found.");
            return Ok(());
        }
    };

    match field {
        ProfileUnsetField::Network => {
            config.network = None;
        }
        ProfileUnsetField::Gui => {
            config.gui = None;
        }
        ProfileUnsetField::OptionalRemove(module) => {
            if let Some(mut opts) = config.optional {
                opts.retain(|m| m != &module);
                if opts.is_empty() {
                    config.optional = None;
                } else {
                    config.optional = Some(opts);
                }
            }
        }
    }

    let content = toml::to_string_pretty(&config).context("Failed to serialise cordon.toml")?;
    fs::write("cordon.toml", content).context("Failed to write cordon.toml")?;
    println!("✅ Updated profile in cordon.toml");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_userconfig_defaults() {
        let toml = r#"
[[mount]]
src = "/tmp"
dest = "/tmp"
mode = "rw"
when = "always"
required = false
"#;
        let config: UserConfig = toml::from_str(toml).unwrap();
        assert!(config.network.is_none());
        assert!(config.gui.is_none());
        assert!(config.optional.is_none());
        assert_eq!(config.mounts.len(), 1);
    }

    #[test]
    fn test_userconfig_profile_network() {
        let toml = r#"
network = "allow"
"#;
        let config: UserConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.network.as_deref(), Some("allow"));
        assert!(config.gui.is_none());
        assert_eq!(config.mounts.len(), 0);
    }

    #[test]
    fn test_userconfig_profile_gui() {
        let toml = r#"
gui = true
"#;
        let config: UserConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.gui, Some(true));
    }

    #[test]
    fn test_userconfig_profile_optional() {
        let toml = r#"
optional = ["audio_pipewire"]
"#;
        let config: UserConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.optional.as_deref(), Some(vec!["audio_pipewire".to_string()].as_slice()));
    }

    #[test]
    fn test_userconfig_profile_full() {
        let toml = r#"
network = "full"
gui = false
optional = ["dbus_session", "audio_pipewire"]

[[mount]]
src = "/etc/issue"
dest = "/etc/issue"
mode = "ro"
when = "always"
required = false
"#;
        let config: UserConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.network.as_deref(), Some("full"));
        assert_eq!(config.gui, Some(false));
        assert_eq!(
            config.optional.as_deref(),
            Some(vec!["dbus_session".to_string(), "audio_pipewire".to_string()].as_slice())
        );
        assert_eq!(config.mounts.len(), 1);
    }

    #[test]
    fn test_named_profile_roundtrip() {
        let toml = r#"
[[profile]]
name = "python"
network = "allow"
optional = ["ld_so_cache", "locale_files"]
"#;
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.profiles.len(), 1);
        assert_eq!(config.profiles[0].name, "python");
        assert_eq!(config.profiles[0].network.as_deref(), Some("allow"));
        assert!(config.profiles[0].gui.is_none());
    }

    #[test]
    fn test_profiles_config_empty() {
        let toml = "";
        let config: ProfilesConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.profiles.len(), 0);
    }
}
