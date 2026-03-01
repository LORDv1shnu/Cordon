use clap::{Parser, Subcommand};
use anyhow::Result;
use std::process::Command;
use std::env;

#[derive(Parser)]
#[command(name = "cordon")]
#[command(about = "Lightweight filesystem sandbox for Linux", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a command inside the sandbox
    Run {
        /// Command to execute
        #[arg(last = true, required = true)]
        cmd: Vec<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { cmd } => run_sandboxed(cmd)?,
    }

    Ok(())
}

fn run_sandboxed(cmd: Vec<String>) -> Result<()> {
    println!("🔒 Running inside sandbox...");
    
    // Get current working directory
    let project_dir = env::current_dir()?;
    let project_path = project_dir.to_str().unwrap();
    let src_dir = project_dir.join("src");
    let has_src = src_dir.exists() && src_dir.is_dir();
    
    if has_src {
        println!("🔒 Protecting src/ as read-only");
    }
    
    println!("📂 Project dir: {}", project_path);
    
    let mut bwrap = Command::new("bwrap");
    bwrap
        .arg("--unshare-all")
        .arg("--ro-bind").arg("/usr").arg("/usr")
        .arg("--ro-bind").arg("/bin").arg("/bin")
        .arg("--ro-bind").arg("/lib").arg("/lib")
        .arg("--ro-bind").arg("/lib64").arg("/lib64")
        .arg("--tmpfs").arg("/tmp")
        .arg("--proc").arg("/proc")
        .arg("--dev").arg("/dev")
        
        // Bind project directory as WRITABLE
        .arg("--bind").arg(project_path).arg(project_path);

    // If src exists, overlay it as read-only
    if has_src {
        let src_path = src_dir.to_str().unwrap();
        bwrap
            .arg("--ro-bind").arg(src_path).arg(src_path);
    }

    // Set working directory inside sandbox
    bwrap
        .arg("--chdir").arg(project_path)
        .arg("--");

    bwrap.args(&cmd);
    
    let status = bwrap.status()?;
    
    if status.success() {
        println!("✅ Command completed successfully");
    } else {
        println!("❌ Command failed with status: {}", status);
    }
    
    Ok(())
}