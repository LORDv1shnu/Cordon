use clap::Parser;

/// Cordon exit codes
///
/// | Code | Meaning                                                         |
/// |------|-----------------------------------------------------------------|
/// | 0    | Success — sandboxed command exited 0                            |
/// | 1    | Cordon internal error (scan failure, config error, etc.)        |
/// | 2    | Cordon usage error (bad CLI args)                               |
/// | 125  | Cordon could not set up the sandbox (bwrap not found, etc.)     |
/// | 126  | Sandboxed command found but not executable                      |
/// | 127  | Sandboxed command not found inside sandbox                      |
/// | N    | Any other code: forwarded directly from the sandboxed process   |
pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const INTERNAL_ERROR: i32 = 1;
    pub const USAGE_ERROR: i32 = 2;
    pub const SANDBOX_SETUP_FAILED: i32 = 125;
    pub const COMMAND_NOT_EXECUTABLE: i32 = 126;
    pub const COMMAND_NOT_FOUND: i32 = 127;
}

// CLI definitions: argument structs, subcommands, flags
mod cli;

// Sandbox runner: builds and spawns the bwrap process
mod sandbox;

// Scanner: full_scan() and integrity_check()
mod scanner;

// Config: structs, system.toml read/write, cordon.toml (user.toml) discovery
mod config;

// Structured error types
mod errors;

// Logging initialisation (tracing framework)
mod logger;

use cli::{Cli, Commands};
use errors::CordonError;

/// Entry point. Parses CLI and dispatches to the appropriate module.
/// main.rs intentionally contains no business logic — it only routes.
fn main() {
    let cli = Cli::parse();

    let result: anyhow::Result<()> = match cli.command {
        Commands::Run {
            cmd,
            net,
            domains,
            debug,
            dry_run,
            gui,
            optional,
        } => {
            // Initialise logging before doing anything else so that every
            // subsequent tracing macro call is captured.
            if let Err(e) = logger::init_logging(debug) {
                eprintln!("critical: failed to initialise logger: {e}");
                std::process::exit(exit_codes::INTERNAL_ERROR);
            }

            sandbox::run_sandboxed(cmd, net, domains, dry_run, gui, optional).map_err(Into::into)
        }

        Commands::Scan {} => {
            // scanner::full_scan() prints its own header — no need to print here
            scanner::full_scan().map_err(Into::into)
        }

        Commands::Add { path, mode } => config::add_user_mount(path, mode).map_err(Into::into),

        Commands::Remove { path } => config::remove_user_mount(path).map_err(Into::into),

        Commands::Edit {} => config::edit_user_config().map_err(Into::into),
    };

    if let Err(e) = result {
        // Check if it is a typed CordonError — if so, render a premium diagnostic box.
        if let Some(cordon_err) = e.downcast_ref::<CordonError>() {
            print_diagnostic_box(cordon_err);
            let exit_code = extract_exit_code_from_cordon(cordon_err);
            print_failure_reason(cordon_err, exit_code);

            // Use the structured exit code where possible.
            let code = match cordon_err {
                CordonError::ExecutionError(c) => *c,
                CordonError::CommandNotFound(_) => exit_codes::COMMAND_NOT_FOUND,
                CordonError::PermissionDenied(_) => exit_codes::COMMAND_NOT_EXECUTABLE,
                CordonError::DependencyMissing(_) => exit_codes::SANDBOX_SETUP_FAILED,
                CordonError::NamespaceError(_) => exit_codes::SANDBOX_SETUP_FAILED,
                _ => exit_codes::INTERNAL_ERROR,
            };
            std::process::exit(code);
        }

        // Fall back to plain error rendering for non-CordonError anyhow errors.
        eprintln!("error: {e:#}");
        std::process::exit(exit_codes::INTERNAL_ERROR);
    } else {
        eprintln!("\x1b[90m[CORDON] sandbox exited cleanly — cage destroyed\x1b[0m");
    }
}

/// Renders an ASCII diagnostic box for Cordon failures — matching LION's style.
fn print_diagnostic_box(err: &CordonError) {
    eprintln!("\n+----------------------------------------------------------+");
    eprintln!("|  CORDON ERROR                                            |");
    eprintln!("+----------------------------------------------------------+");

    // Wrap each line of the error message neatly inside the box.
    let msg = format!("{}", err);
    for line in msg.lines() {
        eprintln!("| {:<56} |", line);
    }

    // Extra hint for permission-denied errors.
    if let CordonError::PermissionDenied(path) = err {
        eprintln!("+----------------------------------------------------------+");
        eprintln!("| Try: chmod +x {:<42} |", path);
    }

    eprintln!("+----------------------------------------------------------+");
    eprintln!("| See full log: ~/.config/cordon/logs/last-run.log         |");
    eprintln!("+----------------------------------------------------------+");
}

/// Prints a one-line human-readable hint after the diagnostic box.
fn print_failure_reason(err: &CordonError, exit_code: Option<i32>) {
    let reason: std::borrow::Cow<str> = match err {
        CordonError::CommandNotFound(_) => {
            "binary not found inside sandbox — check the command name".into()
        }
        CordonError::PermissionDenied(_) => "executable permission missing".into(),
        CordonError::DependencyMissing(dep) => {
            format!("'{}' is not installed — install it and retry", dep).into()
        }
        CordonError::NamespaceError(_) => {
            "bubblewrap cannot create user namespaces — run 'cordon scan' or check AppArmor".into()
        }
        CordonError::ScanRequired => "run 'cordon scan' to initialise sandbox configuration".into(),
        CordonError::ExecutionError(_) => match exit_code {
            // curl / wget: couldn't resolve host — almost always means no network.
            Some(6) => {
                "couldn't resolve host — sandbox network is disabled. Try: --net=allow or --net=full"
                    .into()
            }
            // curl: failed to connect.
            Some(7) => "connection refused or unreachable — try: --net=full".into(),
            // curl: SSL/TLS error.
            Some(35) => "SSL handshake failed inside sandbox — try: --net=full".into(),
            Some(code) => format!("command exited with code {code} (check program output above)").into(),
            None => "command failed inside sandbox".into(),
        },
        CordonError::Internal(_) => "internal sandbox setup failure".into(),
    };
    eprintln!("(reason: {})", reason);
}

/// Extracts the raw integer exit code from an `ExecutionError`.
fn extract_exit_code_from_cordon(err: &CordonError) -> Option<i32> {
    if let CordonError::ExecutionError(code) = err {
        Some(*code)
    } else {
        None
    }
}
