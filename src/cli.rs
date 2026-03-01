use clap::{Parser, Subcommand};

/// Top-level CLI entry point.
/// All subcommands and global flags are defined here.
/// This module owns nothing except argument structure — no logic lives here.
#[derive(Parser)]
#[command(name = "cordon")]
#[command(about = "Lightweight filesystem sandbox for Linux", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a command inside a bubblewrap sandbox.
    ///
    /// System directories are mounted read-only.
    /// The current project directory is writable.
    /// src/ (if present) is protected as read-only.
    /// Network is disabled by default.
    ///
    /// Example: cordon run -- npm install
    /// Example: cordon run --network -- curl https://example.com
    Run {
        /// The command to execute inside the sandbox.
        /// Must come after `--` to separate cordon flags from the command.
        #[arg(last = true, required = true)]
        cmd: Vec<String>,

        /// Allow network access inside the sandbox.
        /// By default, network is fully isolated (--unshare-net).
        /// When enabled, /etc and /run are mounted read-only to allow DNS resolution.
        /// Note: /etc/resolv.conf on systemd systems is a symlink into /run/systemd/resolve/.
        #[arg(long, default_value_t = false)]
        network: bool,
    },

    // Phase 2: scan the system and generate ~/.config/cordon/system.toml
    // Scan {},

    // Phase 2: add a path to the per-project cordon.toml (user.toml)
    // Add {
    //     path: String,
    //     #[arg(long, default_value = "ro")]
    //     mode: String,
    // },
}
