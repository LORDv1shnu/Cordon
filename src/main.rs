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

use cli::{Cli, Commands};

/// Entry point. Parses CLI and dispatches to the appropriate module.
/// main.rs intentionally contains no business logic — it only routes.
///
/// Exit code contract:
/// - Cordon setup/internal errors → exit_codes::INTERNAL_ERROR (1)
/// - Sandbox setup failures       → exit_codes::SANDBOX_SETUP_FAILED (125)
/// - Sandboxed process exit code  → forwarded as-is
fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Run { cmd, network, dry_run, gui, optional } => {
            sandbox::run_sandboxed(cmd, network, dry_run, gui, optional)
        }

        Commands::Scan {} => {
            // scanner::full_scan() prints its own header — no need to print here
            scanner::full_scan()
        }

        Commands::Add { path, mode } => {
            config::add_user_mount(path, mode)
        }
    };

    if let Err(e) = result {
        // Check if this is a sandbox exit code error propagated upward
        if let Some(code) = extract_exit_code(&e) {
            std::process::exit(code);
        }
        eprintln!("error: {e:#}");
        std::process::exit(exit_codes::INTERNAL_ERROR);
    }
}

/// Extracts a forwarded process exit code from an anyhow error chain,
/// if the error was produced by sandbox::run_sandboxed propagating it.
fn extract_exit_code(e: &anyhow::Error) -> Option<i32> {
    // sandbox.rs encodes the child exit code in the error message as
    // "exit code: N" so we can recover and forward it here.
    let msg = format!("{e}");
    if let Some(rest) = msg.strip_prefix("exit code: ") {
        rest.trim().parse::<i32>().ok()
    } else {
        None
    }
}
