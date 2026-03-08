use std::process::Command;
use crate::config::SystemConfig;

pub fn apply_system_mounts(
    bwrap: &mut Command,
    system_config: &SystemConfig,
    network: bool,
    gui: bool,
    optional: &[String]
) {
    // --- Apply dynamic mounts from system.toml ---
    for mount in &system_config.mounts {
        if !mount.verified { continue; } // skip unverified modules
        if mount.when == "network" && !network { continue; }
        if mount.when == "gui" && !gui { continue; }
        if mount.when == "optional" {
            if !optional.contains(&mount.name) { continue; }
            if !mount.verified {
                eprintln!("warning: --opt-in {} requested but module is unverified — skipping", mount.name);
                continue;
            }
        }

        let arg_flag = format!("--{}", mount.bind_type);
        bwrap.arg(&arg_flag).arg(&mount.src).arg(&mount.dest);
    }
}

pub fn apply_user_mounts(bwrap: &mut Command, dry_run: bool) {
    // --- Apply dynamic mounts from user.toml (with confirmation) ---
    if let Ok(Some(ref user_config)) = crate::config::find_user_config() {
        let apply = if dry_run {
            // In dry-run mode, always include user.toml mounts so the full command is visible
            true
        } else {
            // Ask user before exposing anything from cordon.toml
            println!();
            println!("⚠️  cordon.toml found with custom path exposures.");
            loop {
                print!("   Apply these mounts? [Enter=yes / N=no / D=show paths]: ");
                std::io::Write::flush(&mut std::io::stdout()).unwrap_or(());
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).unwrap_or(0);
                match input.trim().to_uppercase().as_str() {
                    "" | "Y" => break true,
                    "N" => {
                        println!("   Skipping cordon.toml mounts.");
                        break false;
                    }
                    "D" => {
                        println!("   Paths in cordon.toml:");
                        for m in &user_config.mounts {
                            println!("     {} {} ({})", if m.mode == "rw" { "rw" } else { "ro" }, m.src, m.dest);
                        }
                        // loop again to ask
                    }
                    _ => {
                        println!("   Unknown input. Enter, N, or D.");
                    }
                }
            }
        };

        if apply {
            for mount in &user_config.mounts {
                let arg_flag = if mount.mode == "rw" { "--bind" } else { "--ro-bind" };
                bwrap.arg(arg_flag).arg(&mount.src).arg(&mount.dest);
            }
        }
    }
}
