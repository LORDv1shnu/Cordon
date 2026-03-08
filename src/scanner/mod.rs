//! scanner module
//!
//! Two-mode scanner architecture:
//!
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │  FULL SCAN  (`full_scan`)                                           │
//! │                                                                     │
//! │  Interactive. Run on first use or via `cordon scan`.                │
//! │  Phase 1 — Mandatory (always) modules scanned automatically.        │
//! │  Phase 2 — Asks: "Include network support?"                         │
//! │  Phase 3 — Asks: "Include GUI support?"                             │
//! │  Phase 4 — Lists optional modules one by one, user opts in/out.     │
//! │  Writes result to system.toml.                                      │
//! └─────────────────────────────────────────────────────────────────────┘
//!
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │  INTEGRITY CHECK  (`integrity_check`)                               │
//! │                                                                     │
//! │  Non-interactive. Runs before every `cordon run`.                   │
//! │  Only checks what is already in system.toml — nothing more.         │
//! │  Step 1 — Parse system.toml (malformed → trigger full scan).        │
//! │  Step 2 — Version check (mismatch → trigger full scan).             │
//! │  Step 3 — Foreign entry check (block if found).                     │
//! │  Step 4 — File existence check for each verified mount.             │
//! │  Step 5 — Hard fail if --network modules are missing/unverified.    │
//! │  Step 6 — Hard fail if --gui modules are missing/unverified.        │
//! │  Returns SystemConfig on success, error on failure.                 │
//! └─────────────────────────────────────────────────────────────────────┘

pub mod env_resolver;
pub mod module_scan;
pub mod full_scan;
pub mod integrity;

pub use full_scan::full_scan;
pub use integrity::integrity_check;

// Embedded blueprint — compiled into the binary at build time.
// Cannot be tampered with at runtime. Any change requires a rebuild.
pub(crate) const CORE_TOML: &str = include_str!("../../config/core.toml");