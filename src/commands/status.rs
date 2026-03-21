//! commands/status.rs
//!
//! `cordon status` — show the contents of system.toml without scanning.
//!
//! Displays:
//!   - Header: last_scan timestamp, cordon_version
//!   - One row per mount entry: name, verified (✅/⚠️), bind_type, when, src path
//!
//! Does NOT trigger a scan. Read-only inspection of what has already been verified.

use anyhow::Result;

pub fn run_status() -> Result<()> {
    let system_path = crate::config::get_config_dir()?.join("system.toml");

    if !system_path.exists() {
        println!("\n \x1b[1;33m⚠  system.toml not found\x1b[0m");
        println!("    Run: \x1b[1mcordon scan\x1b[0m to initialise your sandbox configuration.\n");
        return Ok(());
    }

    let content = std::fs::read_to_string(&system_path)?;
    let system: crate::config::SystemConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            println!("\n \x1b[1;31m✗  system.toml is malformed: {}\x1b[0m", e);
            println!("    Run: \x1b[1mcordon scan\x1b[0m to regenerate it.\n");
            return Ok(());
        }
    };

    // ── Header ────────────────────────────────────────────────────────────────
    println!("\n\x1b[1;96m Cordon — System Status\x1b[0m");
    println!(" {}", "─".repeat(72));
    println!(
        "   \x1b[90mLast scan:  \x1b[0m\x1b[1m{}\x1b[0m",
        system.last_scan
    );
    println!(
        "   \x1b[90mVersion:    \x1b[0m\x1b[1m{}\x1b[0m",
        system.cordon_version
    );
    println!(
        "   \x1b[90mConfig:     \x1b[0m\x1b[90m{}\x1b[0m",
        system_path.display()
    );
    println!(" {}", "─".repeat(72));

    // ── Column headers ────────────────────────────────────────────────────────
    println!(
        "\n  {:<3} {:<22} {:<8} {:<10} source path",
        "", "name", "mode", "when"
    );
    println!("  {}", "·".repeat(70));

    // Track stats per category
    let mut counts = std::collections::HashMap::<&str, (usize, usize)>::new(); // (total, unverified)

    for m in &system.mounts {
        let (verified_icon, name_color) = if m.verified {
            ("\x1b[1;32m✅\x1b[0m", "\x1b[97m")
        } else {
            ("\x1b[1;33m⚠️\x1b[0m ", "\x1b[33m")
        };

        let mode_label = match m.bind_type.as_str() {
            "ro-bind"  => "ro",
            "bind"     => "rw",
            "dev-bind" => "dev",
            "symlink"  => "link",
            other      => other,
        };

        let when_colored = match m.when.as_str() {
            "always"   => "\x1b[97malways\x1b[0m   ",
            "network"  => "\x1b[96mnetwork\x1b[0m  ",
            "gui"      => "\x1b[95mgui\x1b[0m      ",
            "optional" => "\x1b[93moptional\x1b[0m ",
            _          => "\x1b[90mN/A\x1b[0m      ",
        };

        let entry = counts.entry(m.when.as_str()).or_insert((0, 0));
        entry.0 += 1;
        if !m.verified { entry.1 += 1; }

        println!(
            "  {} {}{:<21}\x1b[0m {:<8} {} {}",
            verified_icon,
            name_color,
            m.name,
            mode_label,
            when_colored,
            m.src
        );
    }

    // ── Summary footer ────────────────────────────────────────────────────────
    let total       = system.mounts.len();
    let total_unver = system.mounts.iter().filter(|m| !m.verified).count();
    let total_ver   = total - total_unver;

    println!("  {}", "·".repeat(70));
    println!(
        "\n   \x1b[90m{} module(s) total  —  {} verified  /  {} unverified\x1b[0m",
        total, total_ver, total_unver
    );

    // Per-category breakdown
    let order = ["always", "network", "gui", "optional"];
    let mut parts = Vec::new();
    for cat in &order {
        if let Some((n, u)) = counts.get(cat) {
            if *u > 0 {
                parts.push(format!("\x1b[93m{}:{}/{}\x1b[0m", cat, n - u, n));
            } else {
                parts.push(format!("\x1b[90m{}:{}/{}\x1b[0m", cat, n, n));
            }
        }
    }
    println!("   {}", parts.join("  "));

    if total_unver > 0 {
        println!(
            "\n   \x1b[1;33m⚠  {} module(s) unverified — run: cordon scan\x1b[0m",
            total_unver
        );
    } else {
        println!("\n   \x1b[1;32m✓  All modules verified — sandbox is ready\x1b[0m");
    }

    println!();
    Ok(())
}
