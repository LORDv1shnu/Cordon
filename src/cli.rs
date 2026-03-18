use clap::{Parser, Subcommand};

/// Top-level CLI entry point.
/// All subcommands and global flags are defined here.
/// This module owns nothing except argument structure — no logic lives here.
#[derive(Parser)]
#[command(name = "cordon")]
#[command(version = "0.1.0")] // Added version
#[command(
    about = "Lightweight filesystem sandbox for Linux",
    long_about = "Cordon is a lightweight, per-execution filesystem sandbox for Linux using bubblewrap. \
It allows you to run potentially unsafe scripts or binaries with limited permissions, \
restricting their view of the filesystem while keeping your project directory writable."
)]
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

        /// Network permission profile:
        ///   disable — no network access at all (default)
        ///   allow   — only domains in proxy.toml are reachable
        ///   full    — unrestricted internet access
        #[arg(long, value_name = "PROFILE", default_value = "disable")]
        net: crate::sandbox::network::NetworkMode,

        /// Allow these domains through the network proxy (if active).
        /// Multiple domains can be specified: '--domain google.com --domain github.com' or comma-separated.
        #[arg(long = "domain", value_name = "DOMAIN", value_delimiter = ',')]
        domains: Vec<String>,

        /// Enable detailed technical tracing logs on stderr.
        /// Also writes a full trace to ~/.config/cordon/logs/last-run.log on every run.
        #[arg(long, default_value_t = false)]
        debug: bool,

        /// Show the bubblewrap command and exit without executing it.
        /// For debugging purposes, to see exactly what cordon is doing under the hood.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Enable GUI app support (X11/Wayland/fonts).
        #[arg(long, default_value_t = false)]
        gui: bool,

        /// Activate optional modules by name (e.g. --optional audio --optional dbus).
        /// Module must exist in system.toml and be verified.
        #[arg(long, value_name = "MODULE")]
        optional: Vec<String>,
    },

    /// Scan the system and generate ~/.config/cordon/system.toml.
    Scan {},

    /// Add a custom path to the per-project cordon.toml.
    Add {
        /// Path to the directory or file.
        path: String,
        /// Access mode for the path (ro = read-only, rw = read-write).
        #[arg(long, default_value = "ro")]
        mode: String,
    },

    /// Remove a custom path from the per-project cordon.toml.
    Remove {
        /// Path to remove.
        path: String,
    },

    /// Open the local cordon.toml in the system default editor.
    Edit {},

    /// Check that the sandbox is ready to run: bwrap, namespaces, AppArmor, and modules.
    /// Exits 0 if all checks pass, 1 if any check fails.
    Check,

    /// List all mounts that would be active in the next sandbox run.
    /// Shows system.toml entries and cordon.toml project mounts side-by-side.
    List,
}
