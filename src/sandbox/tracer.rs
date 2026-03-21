use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Command;
use std::collections::HashSet;

/// Prepend strace to the build bwrap command.
/// Redirects strace output to the given temp file.
pub fn wrap_with_strace(bwrap: Command, out_file: &Path) -> Command {
    let original_args = bwrap.get_args().map(|a| a.to_os_string()).collect::<Vec<_>>();
    let program = bwrap.get_program().to_os_string();
    
    let mut strace = Command::new("strace");
    
    // Build new args list: -f -e trace=openat,open,access,stat -o <out_file> -- <bwrap> <bwrap_args...>
    strace.arg("-f")
          .arg("-e")
          .arg("trace=openat,open,access,stat")
          .arg("-o")
          .arg(out_file)
          .arg("--")
          .arg(program)
          .args(&original_args);
          
    // Also copy environments if we set any manually, but builder handles it.
    // Wait, apply_environment modifies `bwrap` with env vars. We need to copy them!
    for (k, v) in bwrap.get_envs() {
        if let Some(v) = v {
            strace.env(k, v);
        } else {
            strace.env_remove(k);
        }
    }
    
    strace
}

/// Parses strace log and returns unique denied paths
pub fn parse_strace_log(path: &Path) -> Result<Vec<String>> {
    let file = File::open(path).context("Failed to open strace log")?;
    let reader = BufReader::new(file);
    let mut denied_paths = HashSet::new();

    for line in reader.lines().map_while(Result::ok) {
        // strace line format example:
        // 12345 openat(AT_FDCWD, "/etc/passwd", O_RDONLY) = -1 ENOENT (No such file or directory)
        // 12345 access("/etc/shadow", R_OK) = -1 EACCES (Permission denied)
        if (line.contains("= -1 ENOENT") || line.contains("= -1 EACCES"))
            && let Some(path) = extract_path(&line) {
                // Ignore pseudo-filesystems and noisy paths
                if !path.starts_with("/sys") 
                    && !path.starts_with("/proc") 
                    && !path.starts_with("/dev") 
                    && !path.starts_with("/run/user") 
                    && !path.starts_with("/tmp") {
                    denied_paths.insert(path);
                }
            }
    }

    let mut paths: Vec<String> = denied_paths.into_iter().collect();
    paths.sort();
    Ok(paths)
}

fn extract_path(line: &str) -> Option<String> {
    // Look for the first quote
    if let Some(start) = line.find('"')
        && let Some(end) = line[start + 1..].find('"') {
            return Some(line[start + 1..start + 1 + end].to_string());
        }
    None
}

pub fn print_trace_report(denied: &[String], log_path: &Path) {
    println!("\n\x1b[1;36m▶ Strace Denied Access Report\x1b[0m");
    println!("\x1b[0;90m  {}\x1b[0m", "─".repeat(62));
    
    if denied.is_empty() {
        println!("  \x1b[1;32m✓\x1b[0m No significant denied accesses caught.");
    } else {
        println!("  The sandboxed application tried to access these paths but could not:");
        for path in denied {
            println!("    \x1b[1;31m✗\x1b[0m {}", path);
        }
        println!();
        println!("  If the application failed to run, it might need one of these paths.");
        println!("  You can pipe this trace directly into cordon add:");
        println!("    \x1b[1mcordon add --from-trace {}\x1b[0m", log_path.display());
    }
    println!("\x1b[0;90m  {}\x1b[0m\n", "─".repeat(62));
}
