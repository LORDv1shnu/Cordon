//! commands/list.rs
//!
//! `cordon list` — show all mounts that would be active in the next sandbox run.
//!
//! Reads system.toml and cordon.toml (if present) without triggering a scan,
//! and displays a formatted table of every mount entry grouped by source:
//!
//!   • System mounts  — from ~/.config/cordon/system.toml  (scanner output)
//!   • User mounts    — from ./cordon.toml                 (per-project)

use anyhow::Result;
use tracing::debug;

pub fn run_list() -> Result<()> {
    println!("\n\x1b[1;96m Cordon — Active Mounts\x1b[0m");
    println!(" {}", "─".repeat(66));

    // ── System mounts (system.toml) ───────────────────────────────────────────
    let system_path = crate::config::get_config_dir()?.join("system.toml");

    if !system_path.exists() {
        println!("\n \x1b[1;33m⚠  system.toml not found — run: cordon scan\x1b[0m\n");
        return Ok(());
    }

    let content = std::fs::read_to_string(&system_path)?;
    let system: crate::config::SystemConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            println!("\n \x1b[1;31m✗  system.toml is malformed: {}\x1b[0m", e);
            println!("    run: cordon scan\n");
            return Ok(());
        }
    };

    debug!("listing system mounts from {}", system_path.display());

    println!(
        "\n \x1b[1;97mSystem mounts\x1b[0m  \x1b[90m(from system.toml — scan: {})\x1b[0m",
        system.last_scan
    );
    println!(
        " {:<22} {:<8} {:<10} {}",
        "name", "mode", "when", "source path"
    );
    println!(" {}", "·".repeat(66));

    let mut always_count = 0usize;
    let mut net_count    = 0usize;
    let mut gui_count    = 0usize;
    let mut opt_count    = 0usize;
    let mut unverified   = 0usize;

    for m in &system.mounts {
        let status = if m.verified {
            "\x1b[32m✓\x1b[0m"
        } else {
            unverified += 1;
            "\x1b[31m✗\x1b[0m"
        };

        // mode abbreviation
        let mode_label = match m.bind_type.as_str() {
            "ro-bind"   => "ro",
            "bind"      => "rw",
            "dev-bind"  => "dev",
            "symlink"   => "link",
            other       => other,
        };

        // when label colouring
        let when_label = match m.when.as_str() {
            "always"   => { always_count += 1; "\x1b[97malways\x1b[0m   " }
            "network"  => { net_count += 1;    "\x1b[96mnetwork\x1b[0m  " }
            "gui"      => { gui_count += 1;    "\x1b[95mgui\x1b[0m      " }
            "optional" => { opt_count += 1;    "\x1b[93moptional\x1b[0m " }
            other      => { let _ = other;     "\x1b[90mN/A\x1b[0m      " }
        };

        println!(
            " {} {:<21} {:<8} {} {}",
            status,
            m.name,
            mode_label,
            when_label,
            m.src
        );
    }

    let total = system.mounts.len();
    println!(" {}", "·".repeat(66));
    println!(
        "   \x1b[90m{total} entries — always:{always_count}  network:{net_count}  gui:{gui_count}  optional:{opt_count}",
    );
    if unverified > 0 {
        println!(
            "   \x1b[1;33m{unverified} unverified (✗) — run: cordon scan to fix\x1b[0m"
        );
    }

    // ── User mounts (cordon.toml) ─────────────────────────────────────────────
    match crate::config::find_user_config()? {
        None => {
            println!("\n \x1b[90mNo cordon.toml found in this directory or its parents.\x1b[0m");
            println!(" \x1b[90mTip: cordon add <path> --mode ro  to create one.\x1b[0m");
        }
        Some(user_cfg) if user_cfg.mounts.is_empty() => {
            println!("\n \x1b[90mcordon.toml found but has no mounts.\x1b[0m");
        }
        Some(user_cfg) => {
            println!(
                "\n \x1b[1;97mProject mounts\x1b[0m  \x1b[90m(from cordon.toml)\x1b[0m"
            );
            println!(
                " {:<30} {:<8} {:<10} {}",
                "source path", "mode", "when", "required"
            );
            println!(" {}", "·".repeat(66));

            for m in &user_cfg.mounts {
                let exists = std::path::Path::new(&m.src).exists();
                let status = if exists { "\x1b[32m✓\x1b[0m" } else { "\x1b[31m✗\x1b[0m" };
                let req = if m.required { "yes" } else { "no" };
                println!(
                    " {} {:<29} {:<8} {:<10} {}",
                    status, m.src, m.mode, m.when, req
                );
            }

            println!(" {}", "·".repeat(66));
            let proj_total = user_cfg.mounts.len();
            let proj_missing = user_cfg.mounts.iter()
                .filter(|m| !std::path::Path::new(&m.src).exists())
                .count();
            println!("   \x1b[90m{proj_total} project mount(s)");
            if proj_missing > 0 {
                println!(
                    "   \x1b[1;33m{proj_missing} path(s) do not exist on disk (✗)\x1b[0m"
                );
            }
        }
    }

    println!();
    Ok(())
}
