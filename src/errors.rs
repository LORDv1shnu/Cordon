//! errors.rs
//!
//! Defines the structured error types for Cordon using `thiserror`.
//!
//! Using typed errors (rather than raw anyhow strings) lets main.rs
//! inspect the *kind* of failure and print actionable hints to the user,
//! instead of just dumping an opaque error chain.

use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)] // variants kept for future use as the codebase grows
pub enum CordonError {
    /// A required external binary (e.g. bwrap) is not installed / not in PATH.
    #[error("dependency missing: {0}")]
    DependencyMissing(String),

    /// The executable the user asked to run could not be located inside the sandbox.
    #[error("command not found: {0}\nThe executable could not be located in PATH inside the sandbox.")]
    CommandNotFound(String),

    /// The binary exists but does not have the executable bit set.
    #[error("permission denied: {0}\nFile exists but is not executable.")]
    PermissionDenied(String),

    /// The sandboxed command ran but exited with a non-zero code.
    #[error("command failed inside sandbox\nexit code: {0}")]
    ExecutionError(i32),

    /// Bubblewrap could not create user namespaces (AppArmor restriction, etc.).
    #[error("namespace setup failed: {0}")]
    NamespaceError(String),

    /// system.toml is missing or stale — the user must run `cordon scan` first.
    #[error("system scan required\nRun 'cordon scan' to initialise your sandbox configuration.")]
    ScanRequired,

    /// Catch-all for internal Cordon setup errors.
    #[error("internal error: {0}")]
    Internal(String),
}

#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, CordonError>;
