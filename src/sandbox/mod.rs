//! sandbox.rs
//!
//! Responsible for building and executing the bubblewrap sandbox.
//!
//! This module:
//! - Loads verified system mounts from system.toml
//! - Loads optional per-project user mounts from cordon.toml
//! - Constructs the final bwrap command
//! - Applies namespace isolation
//! - Applies network policy
//! - Executes the requested command inside the sandbox
//!
//! This module contains NO scanning logic and NO configuration mutation logic.
//! It only consumes already-verified configuration.
//!
//! ## Exit code contract
//! Non-zero exits from the sandboxed process are forwarded via an anyhow error
//! encoded as `"exit code: N"`. `main.rs` decodes this and calls
//! `std::process::exit(N)` so the shell sees the child's real exit code.
//! Sandbox setup failures (bwrap missing, scan error, etc.) produce exit 125.

pub mod builder;
pub mod executor;
pub mod mounts;

pub use executor::run_sandboxed;
