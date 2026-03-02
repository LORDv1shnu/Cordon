use clap::Parser;
use anyhow::Result;

// CLI definitions: argument structs, subcommands, flags
mod cli;

// Sandbox runner: builds and spawns the bwrap process
mod sandbox;

// Phase 2: scanner module
mod scanner;

// Phase 2: config reader/writer for system.toml and user.toml
mod config;

use cli::{Cli, Commands};

/// Entry point. Parses CLI and dispatches to the appropriate module.
/// main.rs intentionally contains no business logic — it only routes.
fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { cmd, network } => sandbox::run_sandboxed(cmd, network)?,
        Commands::Scan {} => scanner::run_scan()?,
        Commands::Add { path, mode } => config::add_user_mount(path, mode)?,
    }

    Ok(())
}
