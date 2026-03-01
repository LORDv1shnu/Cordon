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
        /// Allow network access inside sandbox
        #[arg(long, default_value_t = false)]
        network: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { cmd, network } => run_sandboxed(cmd, network)?,
    }

    Ok(())
}

fn run_sandboxed(cmd: Vec<String>, network: bool) -> Result<()> {
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
        // .arg("--unshare-all")
        .arg("--unshare-user")
        .arg("--unshare-ipc")
        .arg("--unshare-pid")
        .arg("--unshare-uts")
        .arg("--unshare-cgroup")
        .arg("--ro-bind").arg("/usr").arg("/usr")
        .arg("--symlink").arg("usr/bin").arg("/bin")
        .arg("--symlink").arg("usr/lib").arg("/lib")
        .arg("--symlink").arg("usr/lib64").arg("/lib64")
        .arg("--symlink").arg("usr/sbin").arg("/sbin")
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
    
    // Only unshare network if network is disabled
    if !network {
        bwrap.arg("--unshare-net");
        println!("🌐 Network: disabled");
    } else {
        // Need /etc/resolv.conf for DNS resolution (SENSITIVE!)
        bwrap
            .arg("--ro-bind").arg("/etc").arg("/etc") 
            .arg("--ro-bind").arg("/run").arg("/run");
        println!("🌐 Network: enabled");
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