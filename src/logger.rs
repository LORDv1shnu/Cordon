//! logger.rs
//!
//! Initializes the `tracing` diagnostic framework for Cordon.
//!
//! Two sinks are configured on every run:
//!
//!   1. **File**   — `~/.config/cordon/logs/last-run.log`
//!      Always at TRACE level (full detail, never shown on screen).
//!
//!    - Stderr:
//!      • normal run  → INFO  (clean, minimal output)
//!      • --debug run → DEBUG (verbose trace for troubleshooting)

use std::path::PathBuf;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Call once at startup (before any `tracing` macros are used).
///
/// `debug` comes from the `--debug` CLI flag on `cordon run`.
pub fn init_logging(debug: bool, quiet: bool) -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    // ── 1. File sink (full trace, persisted across runs) ─────────────────────
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let log_dir = PathBuf::from(&home).join(".config/cordon/logs");
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::never(&log_dir, "last-run.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // ── 2. Stderr sink (filtered, human-readable) ─────────────────────────────
    let stderr_level = if quiet {
        "error"
    } else if debug {
        "debug"
    } else {
        "info"
    };

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .with_filter(EnvFilter::new(stderr_level));

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_filter(EnvFilter::new("trace"));

    // ── 3. Register both layers globally ─────────────────────────────────────
    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .init();

    Ok(guard)
}
