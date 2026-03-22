//! commands/check.rs
//!
//! `cordon check` — pre-flight health check.
//!
//! Tests every layer of the sandbox stack WITHOUT running anything:
//!   1. Is bwrap installed?
//!   2. Can bwrap actually create a user namespace on this kernel?
//!   3. Is AppArmor blocking userns? (and is the fix known?)
//!   4. Does system.toml exist and is it readable?
//!   5. Are all required "always" modules verified?
//!   6. Are network modules verified?
//!   7. Are GUI modules verified?

use anyhow::Result;
use std::process::Command;
use tracing::debug;

/// Result of a single check item.
enum CheckResult {
    Ok(String),
    Warn(String),
    Fail(String),
}

impl CheckResult {
    fn is_fail(&self) -> bool {
        matches!(self, CheckResult::Fail(_))
    }
    fn label(&self) -> &str {
        match self {
            CheckResult::Ok(_) => "\x1b[1;32m  OK  \x1b[0m",
            CheckResult::Warn(_) => "\x1b[1;33m WARN \x1b[0m",
            CheckResult::Fail(_) => "\x1b[1;31m FAIL \x1b[0m",
        }
    }
    fn msg(&self) -> &str {
        match self {
            CheckResult::Ok(m) | CheckResult::Warn(m) | CheckResult::Fail(m) => m,
        }
    }
}

pub fn run_check() -> Result<()> {
    println!("\n\x1b[1;96m Cordon — Health Check\x1b[0m");
    println!(" {}", "─".repeat(54));

    let mut results: Vec<(&str, CheckResult)> = Vec::new();

    // ── 1. bwrap installed ────────────────────────────────────────────────────
    debug!("check: bwrap installed");
    let bwrap_check = match Command::new("bwrap").arg("--version").output() {
        Ok(o) if o.status.success() => {
            let ver = String::from_utf8_lossy(&o.stdout);
            let ver = ver.trim();
            CheckResult::Ok(format!("bubblewrap found — {}", ver))
        }
        _ => CheckResult::Fail(
            "bwrap not found in PATH\n        install: sudo apt install bubblewrap".to_string(),
        ),
    };
    results.push(("bwrap installed", bwrap_check));

    // ── 2. User namespace creation ───────────────────────────────────────────
    debug!("check: userns creation");
    let userns_check = {
        let out = Command::new("bwrap")
            .args([
                "--unshare-user",
                "--ro-bind", "/", "/",
                "--dev", "/dev",
                "--proc", "/proc",
                "--",
                "true",
            ])
            .output();

        match out {
            Ok(o) if o.status.success() => {
                CheckResult::Ok("user namespaces are available".to_string())
            }
            _ => {
                // Check if AppArmor is blocking
                let aa_blocking = std::fs::read_to_string(
                    "/proc/sys/kernel/apparmor_restrict_unprivileged_userns",
                )
                .unwrap_or_default()
                .trim() == "1";

                if aa_blocking {
                    CheckResult::Fail(
                        "AppArmor is blocking user namespaces\n        fix: sudo cordon install"
                            .to_string(),
                    )
                } else {
                    CheckResult::Fail(
                        "bwrap cannot create user namespaces (check dmesg)".to_string(),
                    )
                }
            }
        }
    };
    results.push(("user namespaces", userns_check));

    // ── 3. AppArmor userns restriction status ────────────────────────────────
    debug!("check: AppArmor restriction flag");
    let aa_restrict_path = "/proc/sys/kernel/apparmor_restrict_unprivileged_userns";
    let aa_check = if std::path::Path::new(aa_restrict_path).exists() {
        let val = std::fs::read_to_string(aa_restrict_path)
            .unwrap_or_default();
        let val = val.trim();
        if val == "0" {
            CheckResult::Ok("AppArmor userns restriction is OFF".to_string())
        } else {
            CheckResult::Warn(format!(
                "AppArmor userns restriction is ON (value={})\n        run: sudo cordon install  (one-time fix)",
                val
            ))
        }
    } else {
        CheckResult::Ok("AppArmor userns restriction not present on this kernel".to_string())
    };
    results.push(("AppArmor userns", aa_check));

    // ── 4. system.toml exists ────────────────────────────────────────────────
    debug!("check: system.toml");
    let system_toml_path = crate::config::get_config_dir()?.join("system.toml");
    let system_check = if system_toml_path.exists() {
        match std::fs::read_to_string(&system_toml_path)
            .ok()
            .and_then(|c| toml::from_str::<crate::config::SystemConfig>(&c).ok())
        {
            Some(cfg) => CheckResult::Ok(format!(
                "system.toml OK — last scan: {}",
                cfg.last_scan
            )),
            None => CheckResult::Fail(
                "system.toml is malformed — run: cordon scan".to_string(),
            ),
        }
    } else {
        CheckResult::Fail("system.toml not found — run: cordon scan".to_string())
    };
    results.push(("system.toml", system_check));

    // ── 5. Seccomp BPF support ───────────────────────────────────────────────
    debug!("check: seccomp support");
    let seccomp_actions_path = "/proc/sys/kernel/seccomp/actions_avail";
    let seccomp_check = if std::path::Path::new(seccomp_actions_path).exists() {
        let avail = std::fs::read_to_string(seccomp_actions_path).unwrap_or_default();
        if avail.contains("errno") && avail.contains("allow") {
            CheckResult::Ok("kernel supports seccomp BPF (errno/allow)".to_string())
        } else {
            CheckResult::Warn("seccomp BPF supported but missing errno/allow actions".to_string())
        }
    } else {
        CheckResult::Warn("seccomp BPF support not detected in kernel".to_string())
    };
    results.push(("seccomp BPF", seccomp_check));

    // ── 5-7: Module checks (only if system.toml parsed OK) ───────────────────
    let system_cfg = std::fs::read_to_string(&system_toml_path)
        .ok()
        .and_then(|c| toml::from_str::<crate::config::SystemConfig>(&c).ok());
    let core_cfg = toml::from_str::<crate::config::CoreConfig>(crate::scanner::CORE_TOML).ok();

    if let (Some(sys), Some(core)) = (system_cfg, core_cfg) {
        // ── 5. Required "always" modules ─────────────────────────────────────
        let required_always: Vec<_> = core.modules.iter()
            .filter(|m| m.when == "always" && m.required)
            .collect();

        let always_fail: Vec<String> = required_always.iter().filter_map(|cm| {
            match sys.mounts.iter().find(|m| m.name == cm.name) {
                None => Some(format!("'{}' missing", cm.name)),
                Some(m) if !m.verified => Some(format!("'{}' unverified", cm.name)),
                _ => None,
            }
        }).collect();

        let always_check = if always_fail.is_empty() {
            let count = required_always.len();
            CheckResult::Ok(format!("all {} required core modules verified", count))
        } else {
            CheckResult::Fail(format!(
                "{} — run: cordon scan",
                always_fail.join(", ")
            ))
        };
        results.push(("core modules", always_check));

        // ── 6. Network modules ────────────────────────────────────────────────
        let net_required: Vec<_> = core.modules.iter()
            .filter(|m| m.when == "network" && m.required)
            .collect();

        let net_ok = net_required.iter().all(|cm| {
            sys.mounts.iter().any(|m| m.name == cm.name && m.verified)
        });
        let net_check = if net_required.is_empty() {
            CheckResult::Ok("no required network modules defined".to_string())
        } else if net_ok {
            CheckResult::Ok(format!("{} network module(s) verified", net_required.len()))
        } else {
            CheckResult::Warn("some network modules unverified — --net=allow/full may fail\n        run: cordon scan".to_string())
        };
        results.push(("network modules", net_check));

        // ── 7. GUI modules ────────────────────────────────────────────────────
        let gui_required: Vec<_> = core.modules.iter()
            .filter(|m| m.when == "gui" && m.required)
            .collect();

        let gui_ok = gui_required.iter().all(|cm| {
            sys.mounts.iter().any(|m| m.name == cm.name && m.verified)
        });
        let gui_check = if gui_required.is_empty() {
            CheckResult::Ok("no required GUI modules defined".to_string())
        } else if gui_ok {
            CheckResult::Ok(format!("{} GUI module(s) verified", gui_required.len()))
        } else {
            CheckResult::Warn("some GUI modules unverified — --gui may fail\n        run: cordon scan".to_string())
        };
        results.push(("GUI modules", gui_check));
    }

    // ── Render results table ──────────────────────────────────────────────────
    println!();
    let mut any_fail = false;
    for (name, result) in &results {
        if result.is_fail() {
            any_fail = true;
        }
        println!(" [{}] {:20} {}", result.label(), name, result.msg());
    }

    // ── Summary line ──────────────────────────────────────────────────────────
    println!();
    let passes = results.iter().filter(|(_, r)| matches!(r, CheckResult::Ok(_))).count();
    let warns  = results.iter().filter(|(_, r)| matches!(r, CheckResult::Warn(_))).count();
    let fails  = results.iter().filter(|(_, r)| matches!(r, CheckResult::Fail(_))).count();
    println!(
        " {} passed  {} warned  {} failed  ({} total)",
        passes, warns, fails, results.len()
    );
    println!(" {}", "─".repeat(54));

    if any_fail {
        println!(" \x1b[1;31m✗ sandbox is NOT ready — fix the above failures first\x1b[0m\n");
        // Return error to allow main to exit with non-zero without printing an error box
        anyhow::bail!("cordon check failed");
    } else if warns > 0 {
        println!(" \x1b[1;33m⚠ sandbox is usable but has warnings\x1b[0m\n");
    } else {
        println!(" \x1b[1;32m✓ sandbox is ready\x1b[0m\n");
    }

    Ok(())
}
