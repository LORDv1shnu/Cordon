use crate::config::{load_profiles, save_profiles, NamedProfile};
use crate::sandbox::network::NetworkMode;
use anyhow::{Context, Result};

pub fn run_create(
    name: String,
    net: Option<NetworkMode>,
    gui: bool,
    optional: Vec<String>,
) -> Result<()> {
    let mut config = load_profiles()?;

    // Remove if already exists to overwrite
    config.profiles.retain(|p| p.name != name);

    let net_str = net.map(|n| match n {
        NetworkMode::Disable => "disable".to_string(),
        NetworkMode::Allow => "allow".to_string(),
        NetworkMode::Full => "full".to_string(),
    });

    let optional_vec = if optional.is_empty() {
        None
    } else {
        Some(optional)
    };

    let gui_opt = if gui { Some(true) } else { None };

    config.profiles.push(NamedProfile {
        name: name.clone(),
        network: net_str,
        gui: gui_opt,
        optional: optional_vec,
    });

    save_profiles(&config)?;
    println!("✅ Profile '{}' saved.", name);
    Ok(())
}

pub fn run_list() -> Result<()> {
    let config = load_profiles()?;
    if config.profiles.is_empty() {
        println!("No saved profiles. Use 'cordon profile create <name>' to add one.");
        return Ok(());
    }

    println!("{:<16}  {:<10}  {:<6}  {}", "NAME", "NET", "GUI", "OPTIONAL");
    println!("{}", "─".repeat(60));

    for p in &config.profiles {
        let net = p.network.as_deref().unwrap_or("—");
        let gui = if p.gui.unwrap_or(false) { "yes" } else { "—" };
        let opt = p
            .optional
            .as_ref()
            .map(|v| v.join(", "))
            .unwrap_or_else(|| "—".to_string());
        println!("{:<16}  {:<10}  {:<6}  {}", p.name, net, gui, opt);
    }
    Ok(())
}

pub fn run_delete(name: String) -> Result<()> {
    let mut config = load_profiles()?;
    let initial_len = config.profiles.len();
    config.profiles.retain(|p| p.name != name);

    if config.profiles.len() == initial_len {
        println!("⚠️ No profile named '{}' found.", name);
        return Ok(());
    }

    save_profiles(&config)?;
    println!("✅ Profile '{}' deleted.", name);
    Ok(())
}

pub fn run_show(name: String) -> Result<()> {
    let config = load_profiles()?;
    for p in &config.profiles {
        if p.name == name {
            let content = toml::to_string_pretty(p).context("Failed to serialize profile")?;
            println!("{}", content.trim());
            return Ok(());
        }
    }
    
    // Check built-in profiles if it's not a saved profile
    let built_in = match name.as_str() {
        "python" => Some(NamedProfile {
            name: "python".to_string(),
            network: Some("allow".to_string()),
            gui: None,
            optional: Some(vec!["ld_so_cache".to_string(), "locale_files".to_string()]),
        }),
        "node" => Some(NamedProfile {
            name: "node".to_string(),
            network: Some("allow".to_string()),
            gui: None,
            optional: Some(vec!["ld_so_cache".to_string(), "home_config".to_string()]),
        }),
        "rust" => Some(NamedProfile {
            name: "rust".to_string(),
            network: Some("allow".to_string()),
            gui: None,
            optional: Some(vec!["ld_so_cache".to_string()]),
        }),
        "gui-app" => Some(NamedProfile {
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
        println!("(built-in profile)\n");
        let content = toml::to_string_pretty(&p).context("Failed to serialize profile")?;
        println!("{}", content.trim());
        return Ok(());
    }
    
    anyhow::bail!("Profile '{}' not found.", name);
}
