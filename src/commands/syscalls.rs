use crate::sandbox::seccomp::{SeccompPreset, BASIC_BLOCKED, STRICT_ALLOWED};
use anyhow::Result;

pub fn run_syscalls(preset: Option<SeccompPreset>) -> Result<()> {
    let preset = preset.unwrap_or(SeccompPreset::Basic);
    
    println!();
    println!("🛡️  Seccomp Preset: \x1b[1;36m{:?}\x1b[0m", preset);
    
    match preset {
        SeccompPreset::Basic => {
            println!("   Action: \x1b[1;33mENOSYS\x1b[0m (Function Not Implemented) on any of these syscalls.");
            println!();
            println!("   \x1b[1;4mSYSCALL\x1b[0m               \x1b[1;4mREASON\x1b[0m");
            for syscall in BASIC_BLOCKED {
                let reason = match *syscall {
                    "ptrace" => "Process inspection / code injection",
                    "process_vm_readv" => "Cross-process memory reads",
                    "process_vm_writev" => "Cross-process memory writes",
                    "userfaultfd" => "Speculative execution exploits",
                    "perf_event_open" => "Side-channel attack surface",
                    "kexec_load" => "Replaces the running kernel",
                    "mount" => "Namespace escape / filesystem modification",
                    "pivot_root" => "Namespace escape primitive",
                    _ => "Security hardening",
                };
                println!("   {:<20} {}", syscall, reason);
            }
        }
        SeccompPreset::Strict => {
            println!("   Action: \x1b[1;32mALLOW\x1b[0m only the syscalls in the strict allow-list.");
            println!("   Everything else returns \x1b[1;33mENOSYS\x1b[0m.");
            println!();
            println!("   Allowed syscalls ({} total):", STRICT_ALLOWED.len());
            
            let mut line = String::from("   ");
            for (i, syscall) in STRICT_ALLOWED.iter().enumerate() {
                line.push_str(syscall);
                if i < STRICT_ALLOWED.len() - 1 {
                    line.push_str(", ");
                }
                if line.len() > 70 {
                    println!("{}", line);
                    line = String::from("   ");
                }
            }
            if !line.trim().is_empty() {
                println!("{}", line);
            }
        }
        SeccompPreset::None => {
            println!("   No seccomp filtering applied. All syscalls allowed by kernel.");
        }
    }
    println!();
    
    Ok(())
}
