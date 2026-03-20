use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};


/// Implements `cordon log [--last N] [--errors]`
pub fn run_log(last: Option<usize>, errors_only: bool) -> Result<()> {
    let log_path = crate::config::get_config_dir()?.join("logs").join("last-run.log");
    
    if !log_path.exists() {
        println!("No last-run.log found at {}", log_path.display());
        return Ok(());
    }
    
    let file = File::open(&log_path).context("Failed to open last-run.log")?;
    let reader = BufReader::new(file);
    
    let mut lines: Vec<String> = Vec::new();
    for line in reader.lines().filter_map(|l| l.ok()) {
        if errors_only {
            if line.contains("ERROR") || line.contains("WARN") {
                lines.push(line);
            }
        } else {
            lines.push(line);
        }
    }
    
    if let Some(n) = last {
        let start = if lines.len() > n { lines.len() - n } else { 0 };
        for line in &lines[start..] {
            println!("{}", line);
        }
    } else {
        for line in lines {
            println!("{}", line);
        }
    }
    
    Ok(())
}
