use crate::config::SystemConfig;
use std::process::Command;

pub fn apply_system_mounts(
    bwrap: &mut Command,
    system_config: &SystemConfig,
    network: bool,
    gui: bool,
    optional: &[String],
) {
    for mount in &system_config.mounts {
        // ── Filter by activation condition
        if mount.when == "network" && !network {
            continue;
        }
        if mount.when == "gui" && !gui {
            continue;
        }
        if mount.when == "optional" && !optional.contains(&mount.name) {
            continue;
        }

        // ── Check verification — after the condition filters so we can print
        //    a useful warning for modules the user explicitly requested
        if !mount.verified {
            if mount.when == "optional" && optional.contains(&mount.name) {
                eprintln!(
                    "warning: optional module '{}' was requested but is unverified — skipping",
                    mount.name
                );
            }
            // Always skip unverified mounts regardless of who triggered them.
            continue;
        }

        let arg_flag = format!("--{}", mount.bind_type);
        bwrap.arg(&arg_flag).arg(&mount.src).arg(&mount.dest);
    }
}

pub fn apply_user_mounts(bwrap: &mut Command, dry_run: bool) {
    // Errors reading cordon.toml are surfaced as warnings, not hard failures.
    // A missing or unreadable cordon.toml simply means no extra mounts.
    let user_config = match crate::config::find_user_config() {
        Ok(Some(cfg)) => cfg,
        Ok(None) => return, // no cordon.toml found up the directory tree
        Err(e) => {
            eprintln!("warning: could not read cordon.toml: {}", e);
            return;
        }
    };

    if user_config.mounts.is_empty() {
        return;
    }

    let apply = if dry_run {
        // In dry-run mode always include cordon.toml mounts so the printed
        // command reflects the full set of arguments that would be used.
        true
    } else {
        // Prompt the user before exposing any extra paths from cordon.toml.
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
                        println!("     [{}]  {}", m.mode, m.src);
                    }
                    // loop continues — ask again
                }
                _ => println!("   Unknown input. Enter=yes, N=no, D=show paths."),
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
