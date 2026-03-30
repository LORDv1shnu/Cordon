use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use anyhow::{Context, Result};

/// Get the path where the wrapper script should be placed.
pub fn get_wrapper_path(cmd: &str) -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME env var not set")?;
    let mut path = PathBuf::from(home);
    path.push(".local/bin");
    path.push(cmd);
    Ok(path)
}

/// Create a wrapper script for the given command.
pub fn wrap(cmd: &str, show: bool) -> Result<()> {
    let script = format!("#!/bin/sh\nexec cordon run -- {} \"$@\"\n", cmd);

    if show {
        println!("{}", script);
        return Ok(());
    }

    let path = get_wrapper_path(cmd)?;
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).context("Failed to create .local/bin directory")?;
        println!("Created directory: {:?}", parent);
        println!("NOTE: Make sure {:?} is in your PATH.", parent);
    }

    fs::write(&path, script).context(format!("Failed to write wrapper script to {:?}", path))?;

    let mut perms = fs::metadata(&path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).context("Failed to set executable permissions on wrapper")?;

    println!("Wrapped '{}' at {:?}", cmd, path);
    Ok(())
}

/// Remove the wrapper script for the given command.
pub fn unwrap(cmd: &str) -> Result<()> {
    let path = get_wrapper_path(cmd)?;
    if path.exists() {
        fs::remove_file(&path).context(format!("Failed to remove wrapper at {:?}", path))?;
        println!("Unwrapped '{}' (removed {:?})", cmd, path);
    } else {
        println!("No wrapper found for '{}' at {:?}", cmd, path);
    }
    Ok(())
}
