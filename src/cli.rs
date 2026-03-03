use clap::{Parser, Subcommand};

/// Top-level CLI entry point.
/// All subcommands and global flags are defined here.
/// This module owns nothing except argument structure — no logic lives here.
#[derive(Parser)]
#[command(name = "cordon")]
#[command(version = "0.1.0")] // Added version
#[command(about = "Lightweight filesystem sandbox for Linux", long_about = "Cordon is a lightweight, per-execution filesystem sandbox for Linux using bubblewrap. \
It allows you to run potentially unsafe scripts or binaries with limited permissions, \
restricting their view of the filesystem while keeping your project directory writable.")]
#[command(arg_required_else_help = true)] // Show help if no subcommand is provided
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a command inside a bubblewrap sandbox.
    ///
    /// System directories (/usr, /bin, /lib) are mounted read-only.
    /// The current project directory is writable.
    /// src/ (if present) is protected as read-only.
    /// Network is disabled by default.
    Run {
        /// The command to execute inside the sandbox.
        /// Must come after `--` to separate cordon flags from the command.
        #[arg(last = true, required = true)]
        cmd: Vec<String>,

        /// Allow network access inside the sandbox.
        /// When enabled, /etc and /run are mounted read-only for DNS resolution.
        #[arg(long, default_value_t = false)]
        network: bool,


        /// Show the bubblewrap command and exit without executing it.
        /// For debugging purposes, to see exactly what cordon is doing under the hood.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    // Phase 2: scan the system and generate ~/.config/cordon/system.toml
    // Scan {},

    /// Add a custom path to the per-project cordon.toml (user.toml).
    /// [PHASE 2 - Planned]
    Add {
        /// Path to the directory or file.
        path: String,
        /// Access mode for the path (ro = read-only, rw = read-write).
        #[arg(long, default_value = "ro")]
        mode: String,
    },
}
