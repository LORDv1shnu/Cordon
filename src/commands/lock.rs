use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use std::io::Read;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockConfig {
    pub cordon_version: String,
    pub timestamp: String,
    pub project_root: PathBuf,
    pub mounts: HashMap<String, String>, // Path -> SHA256
}

/// Calculate SHA-256 of a file.
pub fn calculate_hash(path: &Path) -> Result<String> {
    if !path.exists() {
        anyhow::bail!("Path does not exist: {:?}", path);
    }

    // For directories (e.g. symlinks that are treated as binds), we check metadata
    // but Cordon mostly binds files or system dirs.
    // For now, only hash regular files. For dirs, we might hash the dir structure or just metadata.
    let metadata = fs::metadata(path)?;
    if metadata.is_dir() {
        // Return a mock hash for directories (inode + mtime)
        let mtime = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs();
        return Ok(format!("dir:{}:{}", metadata.len(), mtime));
    }

    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

pub fn run_lock_update(resolved_paths: Vec<PathBuf>) -> Result<()> {
    let mut hashes = HashMap::new();
    
    for path in resolved_paths {
        if let Ok(h) = calculate_hash(&path) {
            hashes.insert(path.to_string_lossy().to_string(), h);
        }
    }

    let lock = LockConfig {
        cordon_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Local::now().to_rfc3339(),
        project_root: std::env::current_dir()?,
        mounts: hashes,
    };

    let content = toml::to_string_pretty(&lock).context("Failed to serialize lockfile")?;
    fs::write("cordon.lock", content).context("Failed to write cordon.lock")?;
    
    info!("✅ Created cordon.lock with {} tracked paths", lock.mounts.len());
    Ok(())
}

pub fn run_lock_verify() -> Result<()> {
    let lock_path = Path::new("cordon.lock");
    if !lock_path.exists() {
        anyhow::bail!("No cordon.lock found in the current directory.");
    }

    let content = fs::read_to_string(lock_path)?;
    let lock: LockConfig = toml::from_str(&content).context("Failed to parse cordon.lock")?;
    
    let mut mismatches = 0;
    for (path_str, expected_hash) in &lock.mounts {
        let path = Path::new(path_str);
        if !path.exists() {
            warn!("MISSING: {}", path_str);
            mismatches += 1;
            continue;
        }

        match calculate_hash(path) {
            Ok(h) if h != *expected_hash => {
                warn!("CHANGED: {}", path_str);
                mismatches += 1;
            }
            Err(e) => {
                warn!("ERROR: {} ({})", path_str, e);
                mismatches += 1;
            }
            _ => {}
        }
    }

    if mismatches > 0 {
        anyhow::bail!("Lock verification failed: {} paths have changed since the lock was created.", mismatches);
    }

    info!("✅ Lock verification passed.");
    println!("OK: Lock verification passed.");
    Ok(())
}
