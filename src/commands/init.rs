use crate::config::UserConfig;
use anyhow::Result;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use tracing::info;

pub fn run_init(yes: bool, force: bool) -> Result<()> {
    let toml_path = Path::new("cordon.toml");
    
    if toml_path.exists() && !force {
        anyhow::bail!("cordon.toml already exists in the current directory. Use --force to overwrite.");
    }

    let project_type = detect_project_type();
    
    let (mut net, mut gui, mut opts) = match project_type {
        Some("rust") => ("allow".to_string(), false, vec!["ld_so_cache".to_string()]),
        Some("node") => ("allow".to_string(), false, vec!["ld_so_cache".to_string(), "home_config".to_string()]),
        Some("python") => ("allow".to_string(), false, vec!["ld_so_cache".to_string(), "locale_files".to_string()]),
        _ => ("disable".to_string(), false, vec![]),
    };

    if let Some(t) = project_type {
        info!("Detected project type: {}", t);
    } else {
        info!("No known project type detected. Using conservative defaults.");
    }

    if !yes {
        println!("Configure cordon sandbox for this project:\n");
        
        let net_prompt = prompt(
            &format!("Network mode [disable, allow, full] (default: {}): ", net),
            &net,
        )?;
        if !net_prompt.is_empty() {
            net = net_prompt;
        }

        let gui_prompt = prompt(
            &format!("GUI support [y/N] (default: {}): ", if gui { "y" } else { "N" }),
            "",
        )?;
        if gui_prompt.eq_ignore_ascii_case("y") || gui_prompt.eq_ignore_ascii_case("yes") {
            gui = true;
        }

        let opts_str = opts.join(",");
        let opts_prompt = prompt(
            &format!("Optional modules (comma-separated log) [none] (default: {}): ", if opts.is_empty() { "none" } else { &opts_str }),
            &opts_str,
        )?;
        
        if opts_prompt.eq_ignore_ascii_case("none") || opts_prompt.is_empty() && opts.is_empty() {
            opts = vec![];
        } else if !opts_prompt.is_empty() && opts_prompt != opts_str {
            opts = opts_prompt.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        }
    }

    let config = UserConfig {
        network: Some(net.clone()),
        gui: if gui { Some(true) } else { None },
        optional: if opts.is_empty() { None } else { Some(opts.clone()) },
        seccomp: None,
        mounts: vec![],
    };

    let toml_string = toml::to_string_pretty(&config)?;
    fs::write(toml_path, toml_string)?;
    
    info!("✅ Written cordon.toml");
    println!("  Network: {}", net);
    println!("  GUI: {}", if gui { "enabled" } else { "disabled" });
    let opts_display = opts.join(", ");
    println!("  Optional modules: {}", if opts.is_empty() { "none" } else { &opts_display });
    
    Ok(())
}

fn detect_project_type() -> Option<&'static str> {
    if Path::new("Cargo.toml").exists() {
        return Some("rust");
    }
    if Path::new("package.json").exists() {
        return Some("node");
    }
    if Path::new("pyproject.toml").exists() || Path::new("setup.py").exists() {
        return Some("python");
    }
    None
}

fn prompt(msg: &str, default: &str) -> std::io::Result<String> {
    print!("{}", msg);
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let trimmed = line.trim().to_string();
    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed)
    }
}
