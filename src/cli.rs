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
        #[arg(long, value_name = "PROFILE")]
        net: Option<crate::sandbox::network::NetworkMode>,

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

        /// Load a named profile from ~/.config/cordon/profiles.toml.
        /// Merged before cordon.toml and CLI flags, so those always take precedence.
        #[arg(long, value_name = "NAME")]
        profile: Option<String>,

        /// Run the sandbox wrapped in strace to catch denied accesses,
        /// and write a report to ~/.config/cordon/logs/last-trace.log.
        #[arg(long, default_value_t = false)]
        trace: bool,

        /// Suppress all Cordon banners and status lines; only show sandboxed command output.
        #[arg(long, default_value_t = false)]
        quiet: bool,

        /// Print every bwrap argument on its own line before executing.
        #[arg(long, default_value_t = false)]
        verbose: bool,

        /// Activate optional modules by name (e.g. --optional audio --optional dbus).
        /// Module must exist in system.toml and be verified.
        #[arg(long, value_name = "MODULE")]
        optional: Vec<String>,

        /// Memory limit (e.g. 512M, 1G). Requires systemd-run.
        #[arg(long, value_name = "SIZE")]
        mem: Option<String>,

        /// CPU limit in number of cores (e.g. 0.5, 2.0). Requires systemd-run.
        #[arg(long, value_name = "N")]
        cpu: Option<f32>,

        /// Maximum number of processes/threads. Requires systemd-run.
        #[arg(long, value_name = "N")]
        pid_limit: Option<u32>,

        /// Execution time limit in seconds. Requires systemd-run.
        #[arg(long, value_name = "SECS")]
        timeout: Option<u64>,

        /// Apply a seccomp syscall filter preset.
        /// Presets: basic (block dangerous), strict (allow-list), none.
        #[arg(long, value_name = "PRESET")]
        seccomp: Option<crate::sandbox::seccomp::SeccompPreset>,
    },

    /// Scan the system and generate ~/.config/cordon/system.toml.
    Scan {
        /// Override distro detection (e.g. --distro nixos).
        #[arg(long, value_name = "NAME")]
        distro: Option<String>,

        /// Non-interactive mode — skip all prompts and use defaults.
        #[arg(long, short = 'y', default_value_t = false)]
        yes: bool,
    },

    /// Scaffold a cordon.toml in the current directory.
    /// Auto-detects project type (Cargo.toml → rust, package.json → node, pyproject.toml → python).
    /// Interactive by default; use --yes to accept all defaults.
    Init {
        /// Skip all prompts and apply detected defaults.
        #[arg(long, short = 'y', default_value_t = false)]
        yes: bool,

        /// Overwrite an existing cordon.toml.
        #[arg(long, default_value_t = false)]
        force: bool,
    },

    /// Add a custom path to the per-project cordon.toml.
    Add {
        /// Path to the directory or file. Optional if --from-trace is provided.
        path: Option<String>,
        /// Access mode for the path (ro = read-only, rw = read-write).
        #[arg(long, default_value = "ro")]
        mode: String,
        /// Add all paths found in the given trace log.
        #[arg(long, value_name = "LOG")]
        from_trace: Option<std::path::PathBuf>,
    },

    /// Remove a custom path from the per-project cordon.toml.
    Remove {
        /// Path to remove.
        path: String,
    },

    /// Open the local cordon.toml in the system default editor.
    Edit {},

    /// Set a default profile field in the per-project cordon.toml.
    #[command(arg_required_else_help = true)]
    Set {
        /// Network permission profile (disable, allow, full).
        #[arg(long, value_name = "PROFILE")]
        net: Option<crate::sandbox::network::NetworkMode>,

        /// Enable GUI app support (X11/Wayland/fonts) by default.
        #[arg(long)]
        gui: bool,

        /// Add an optional module to the default profile.
        #[arg(long, value_name = "MODULE")]
        optional: Option<String>,
    },

    /// Unset a default profile field in the per-project cordon.toml.
    #[command(arg_required_else_help = true)]
    Unset {
        /// Remove network permission profile from defaults.
        #[arg(long)]
        net: bool,

        /// Remove GUI app support from defaults.
        #[arg(long)]
        gui: bool,

        /// Remove an optional module from the default profile.
        #[arg(long, value_name = "MODULE")]
        optional: Option<String>,
    },

    /// Check that the sandbox is ready to run: bwrap, namespaces, AppArmor, and modules.
    /// Exits 0 if all checks pass, 1 if any check fails.
    Check,

    /// List all mounts that would be active in the next sandbox run.
    /// Shows system.toml entries and cordon.toml project mounts side-by-side.
    List,

    /// Show the contents of system.toml without running a scan.
    /// Displays each module's name, verification status, bind type, when category, and source path.
    /// Also shows the last_scan timestamp and cordon_version from the file header.
    /// Useful for debugging "why isn't my module being mounted?" without running any command.
    Status,

    /// Show or tail the access/debug log for the last run.
    Log {
        /// Show only the last N lines.
        #[arg(long, value_name = "N")]
        last: Option<usize>,

        /// Filter for lines containing "ERROR" or "WARN".
        #[arg(long, default_value_t = false)]
        errors: bool,
    },

    /// Deep diagnostic report: kernel, bwrap version, namespaces, AppArmor, environment quirks.
    /// Suggests exact fix commands for each detected problem.
    Doctor,

    /// Manage reusable named sandbox profiles stored in ~/.config/cordon/profiles.toml.
    Profile {
        #[command(subcommand)]
        action: ProfileCommands,
    },

    /// List syscalls blocked or allowed by each seccomp preset.
    Syscalls {
        /// Which preset to display (basic, strict). Default: basic.
        #[arg(long, value_name = "PRESET")]
        preset: Option<crate::sandbox::seccomp::SeccompPreset>,
    },
}

#[derive(Subcommand)]
pub enum ProfileCommands {
    /// Create or overwrite a named profile.
    Create {
        name: String,
        /// Network permission profile (disable, allow, full).
        #[arg(long, value_name = "PROFILE")]
        net: Option<crate::sandbox::network::NetworkMode>,
        /// Enable GUI app support (X11/Wayland/fonts).
        #[arg(long)]
        gui: bool,
        /// Add an optional module to the profile.
        #[arg(long, value_name = "MODULE")]
        optional: Vec<String>,
    },
    /// List all saved profiles in a table.
    List,
    /// Delete a named profile by name.
    Delete { name: String },
    /// Show a single profile's fields.
    Show { name: String },
}
