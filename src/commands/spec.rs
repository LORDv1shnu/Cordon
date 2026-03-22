use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use tracing::info;
use crate::config::UserConfig;
use crate::sandbox::executor::SandboxOptions;

pub fn run_export(profile: Option<String>) -> Result<()> {
    let opts = SandboxOptions {
        cmd: vec!["true".to_string()],
        net: crate::sandbox::network::NetworkMode::Disable,
        domains: vec![],
        dry_run: true,
        gui: false,
        optional: vec![],
        profile,
        trace: false,
        quiet: true,
        verbose: false,
        net_is_explicit: false,
        mem: None,
        cpu: None,
        pid_limit: None,
        timeout: None,
        seccomp: None,
    };

    let (net, gui, optional, seccomp, _paths) = crate::sandbox::executor::resolve_effective_flags(opts)?;

    // We export as a UserConfig-compatible structure that represents the FULL resolved state
    let mut export_config = UserConfig::default();
    export_config.network = Some(match net {
        crate::sandbox::network::NetworkMode::Disable => "disable".to_string(),
        crate::sandbox::network::NetworkMode::Allow => "allow".to_string(),
        crate::sandbox::network::NetworkMode::Full => "full".to_string(),
    });
    export_config.gui = Some(gui);
    export_config.optional = if optional.is_empty() { None } else { Some(optional) };
    export_config.seccomp = seccomp.map(|s| match s {
        crate::sandbox::seccomp::SeccompPreset::Basic => "basic".to_string(),
        crate::sandbox::seccomp::SeccompPreset::Strict => "strict".to_string(),
        crate::sandbox::seccomp::SeccompPreset::None => "none".to_string(),
    });

    // We don't export system paths as user mounts (they are handled by system.toml on the target machine),
    // but we COULD. For now, portable spec means "shared config + explicit user mounts".
    if let Ok(Some(cfg)) = crate::config::find_user_config() {
        export_config.mounts = cfg.mounts;
    }

    let json = serde_json::to_string_pretty(&export_config).context("Failed to serialize spec to JSON")?;
    println!("{}", json);
    Ok(())
}

pub fn run_import(file: PathBuf) -> Result<()> {
    if !file.exists() {
        anyhow::bail!("Spec file not found: {:?}", file);
    }

    let content = fs::read_to_string(&file).context("Failed to read spec file")?;
    let config: UserConfig = serde_json::from_str(&content).context("Failed to parse JSON spec")?;

    let toml_content = toml::to_string_pretty(&config).context("Failed to serialize imported config to TOML")?;
    fs::write("cordon.toml", toml_content).context("Failed to write imported cordon.toml")?;

    info!("✅ Imported sandbox spec from {:?} into cordon.toml", file);
    Ok(())
}
