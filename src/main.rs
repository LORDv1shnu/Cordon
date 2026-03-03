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
        Commands::Run { cmd, network, dry_run, gui } => {
            sandbox::run_sandboxed(cmd, network, dry_run, gui)?
        }

        Commands::Scan {} => {
            // scanner::run_scan() prints its own header — no need to print here
            scanner::run_scan()?;
        }

        Commands::Add { path, mode } => {
            println!("📂 Phase 2: Add command is not yet implemented. (path={}, mode={})", path, mode);
        }
    }

    Ok(())
}
