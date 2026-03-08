use anyhow::{Context, Result};
use crate::config::{CoreConfig, CoreModule, MountEntry, SystemConfig};
use std::io::{self, Write};
use chrono::Utc;
use super::module_scan::scan_module_interactive;
use super::CORE_TOML;

/// Print a yes/no prompt and return true only if the user types 'y' or 'Y'.
/// Any other input (including pressing Enter directly) defaults to No.
fn ask_yes_no(prompt: &str) -> bool {
    print!("{} [y/N]: ", prompt);
    io::stdout().flush().unwrap_or(());
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap_or(0);
    input.trim().eq_ignore_ascii_case("y")
}

/// Full system scan — interactive.
///
/// This is the only function that WRITES to system.toml.
/// Run it once on first use, or again via `cordon scan` when needed.
///
/// Design principle: system.toml should only contain what THIS user,
/// on THIS machine, for THEIR use case actually needs.
/// We never silently add things the user didn't ask for.
///
/// Phase 1: Mandatory (when = "always") — scanned automatically.
///   No choice here. Every sandbox needs /usr, /bin, /lib, etc.
///
/// Phase 2: Network (when = "network") — single yes/no per feature group.
///   Only add if user plans to use --network. Fewer mounts = less exposure.
///
/// Phase 3: GUI (when = "gui") — single yes/no per feature group.
///   Only add if user plans to run GUI apps. Same reasoning.
///
/// Phase 4: Optional (when = "optional") — asked per-module with explanation.
///   User sees what each module does before deciding.
pub fn full_scan() -> Result<()> {
    println!();
    println!("╔══════════════════════════════════════════╗");
    println!("║       Cordon — Full System Scan          ║");
    println!("╚══════════════════════════════════════════╝");
    println!();
    println!("Verifies which system paths exist on this machine");
    println!("and writes system.toml that the sandbox reads at runtime.");
    println!();

    let core: CoreConfig = toml::from_str(CORE_TOML)
        .context("Failed to parse embedded core.toml")?;

    let mut mounts: Vec<MountEntry> = Vec::new();

    // ── Phase 1: Always-mandatory modules (no prompting)
    println!("[ Phase 1: Mandatory system modules ]");
    println!("  Required for any binary to execute. Not skippable.");
    println!();

    for module in core.modules.iter().filter(|m| m.when == "always") {
        print!("  Scanning {:20} ... ", module.name);
        io::stdout().flush().unwrap_or(());
        if let Some(mount) = scan_module_interactive(module)? {
            println!("{}", if mount.verified { "✅" } else { "⚠️  unverified" });
            mounts.push(mount);
        }
    }
    println!();

    // ── Phase 2: Network modules — one choice covers the whole group
    println!("[ Phase 2: Network support ]");
    println!("  Needed for: DNS resolution, HTTPS, curl, npm install, pip.");
    println!("  Skip if you only run offline tools — fewer mounts is safer.");
    println!();

    if ask_yes_no("  Include network support? (for --network flag)") {
        println!();
        for module in core.modules.iter().filter(|m| m.when == "network") {
            print!("  Scanning {:20} ... ", module.name);
            io::stdout().flush().unwrap_or(());
            if let Some(mount) = scan_module_interactive(module)? {
                println!("{}", if mount.verified { "✅" } else { "⚠️  unverified" });
                mounts.push(mount);
            }
        }
    } else {
        println!("  Skipped. Re-run `cordon scan` later to add network support.");
    }
    println!();

    // ── Phase 3: GUI modules — one choice covers the whole group
    println!("[ Phase 3: GUI support ]");
    println!("  Needed for: X11 apps, Wayland apps, GTK/Qt programs.");
    println!("  Skip if you only run CLI tools.");
    println!();

    if ask_yes_no("  Include GUI support? (for --gui flag)") {
        println!();
        for module in core.modules.iter().filter(|m| m.when == "gui") {
            print!("  Scanning {:20} ... ", module.name);
            io::stdout().flush().unwrap_or(());
            if let Some(mount) = scan_module_interactive(module)? {
                println!("{}", if mount.verified { "✅" } else { "⚠️  unverified" });
                mounts.push(mount);
            }
        }
    } else {
        println!("  Skipped. Re-run `cordon scan` later to add GUI support.");
    }
    println!();

    // ── Phase 4: Optional modules — explained and asked individually
    let optionals: Vec<&CoreModule> = core.modules.iter()
        .filter(|m| m.when == "optional")
        .collect();

    if !optionals.is_empty() {
        println!("[ Phase 4: Optional modules ]");
        println!("  None of these are needed for basic CLI sandbox operation.");
        println!("  Only include what you actually need.\n");

        for module in optionals {
            // Show what this module is and what breaks without it, THEN ask.
            // Learning principle: explain before asking, not after.
            println!("  ┌─ {} ─", module.name);
            println!("  │  What: {}", module.description);
            println!("  │  Without it: {}", module.functionality);

            if ask_yes_no("  └  Include this?") {
                print!("     Scanning ... ");
                io::stdout().flush().unwrap_or(());
                if let Some(mount) = scan_module_interactive(module)? {
                    println!("{}", if mount.verified { "✅" } else { "⚠️  unverified" });
                    mounts.push(mount);
                }
            }
            println!();
        }
    }

    // ── Write system.toml
    let system_config = SystemConfig {
        last_scan: Utc::now().to_rfc3339(),
        cordon_version: env!("CARGO_PKG_VERSION").to_string(),
        mounts,
    };

    crate::config::save_system_config(&system_config)?;
    println!("✅ system.toml written. You can now run: cordon run -- <cmd>");
    println!();
    Ok(())
}
