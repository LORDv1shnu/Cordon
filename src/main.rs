use clap::{Parser, error::ErrorKind};

/// Cordon exit codes
///
/// | Code | Meaning                                                         |
/// |------|-----------------------------------------------------------------|
/// | 0    | Success — sandboxed command exited 0                            |
/// | 1    | Cordon internal error (scan failure, config error, etc.)        |
/// | 2    | Cordon usage error (bad CLI args)                               |
/// | 125  | Cordon could not set up the sandbox (bwrap not found, etc.)     |
/// | 126  | Sandboxed command found but not executable                      |
/// | 127  | Sandboxed command not found inside sandbox                      |
/// | N    | Any other code: forwarded directly from the sandboxed process   |
pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const INTERNAL_ERROR: i32 = 1;
    pub const USAGE_ERROR: i32 = 2;
    pub const SANDBOX_SETUP_FAILED: i32 = 125;
    pub const COMMAND_NOT_EXECUTABLE: i32 = 126;
    pub const COMMAND_NOT_FOUND: i32 = 127;
}

// CLI definitions: argument structs, subcommands, flags
mod cli;

pub mod distro;

// Sandbox runner: builds and spawns the bwrap process
mod sandbox;

// Scanner: full_scan() and integrity_check()
mod scanner;

// Config: structs, system.toml read/write, cordon.toml (user.toml) discovery
mod config;

// Structured error types
mod errors;

// Logging initialisation (tracing framework)
mod logger;

// Standalone subcommand implementations (check, list, status …)
mod commands;

// Smart "did you mean?" suggestions and usage hints
mod suggestions;

use cli::{Cli, Commands};
use errors::CordonError;

/// Entry point. Parses CLI and dispatches to the appropriate module.
/// main.rs intentionally contains no business logic — it only routes.
fn main() {
    // Use try_parse so we can intercept clap errors and provide
    // "did you mean?" suggestions and full command syntax hints.
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            match e.kind() {
                // Unknown / misspelled subcommand ─────────────────────────────
                ErrorKind::InvalidSubcommand => {
                    // The bad token is the first user-supplied arg after "cordon".
                    // Pulling it from raw args is more reliable than parsing the
                    // clap ContextValue enum across API versions.
                    let bad = std::env::args()
                        .nth(1)
                        .unwrap_or_default();
                    suggestions::print_unknown_command_error(&bad);
                    std::process::exit(exit_codes::USAGE_ERROR);
                }

                // Missing required positional arg or subcommand ───────────────
                ErrorKind::MissingRequiredArgument
                | ErrorKind::MissingSubcommand => {
                    // The subcommand is the first arg the user gave us.
                    let subcommand_raw = std::env::args().nth(1);
                    let subcommand = subcommand_raw.as_deref();

                    // Extract missing-arg name from the clap ContextValue enum.
                    // ContextValue::String holds the arg name in this case.
                    let missing: String = e
                        .context()
                        .find_map(|(kind, val)| {
                            use clap::error::{ContextKind, ContextValue};
                            if kind == ContextKind::InvalidArg {
                                if let ContextValue::String(s) = val {
                                    return Some(s.clone());
                                }
                                if let ContextValue::Strings(ss) = val {
                                    return Some(ss.join(", "));
                                }
                            }
                            None
                        })
                        .unwrap_or_else(|| "<argument>".to_owned());

                    suggestions::print_missing_arg_error(&missing, subcommand);
                    std::process::exit(exit_codes::USAGE_ERROR);
                }

                // Help and Version: exit 0 ────────────────────────────────────
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    e.print().expect("failed to print help/version");
                    std::process::exit(exit_codes::SUCCESS);
                }

                // All other clap errors: clap's default rendering ─────────────
                _ => {
                    e.print().expect("failed to print clap error");
                    std::process::exit(exit_codes::USAGE_ERROR);
                }
            }
        }
    };

    let is_quiet = matches!(&cli.command, Commands::Run { quiet: true, .. });

    let result: anyhow::Result<()> = match cli.command {
        Commands::Run {
            cmd,
            net,
            domains,
            debug,
            dry_run,
            gui,
            optional,
            profile,
            trace,
            quiet,
            verbose,
            mem,
            cpu,
            pid_limit,
            timeout,
        } => {
            // Initialise logging before doing anything else so that every
            // subsequent tracing macro call is captured.
            if let Err(e) = logger::init_logging(debug, quiet) {
                eprintln!("critical: failed to initialise logger: {e}");
                std::process::exit(exit_codes::INTERNAL_ERROR);
            }

            let net_val = net.unwrap_or(crate::sandbox::network::NetworkMode::Disable);
            sandbox::run_sandboxed(
                cmd,
                net_val,
                domains,
                dry_run,
                gui,
                optional,
                profile,
                trace,
                quiet,
                verbose,
                net.is_some(),
                mem,
                cpu,
                pid_limit,
                timeout,
            )
            .map_err(Into::into)
        }

        Commands::Scan { distro } => {
            // scanner::full_scan() prints its own header — no need to print here
            let distro_override = match distro.as_deref() {
                Some("nixos") => Some(crate::distro::Distro::NixOS),
                Some(_) => Some(crate::distro::Distro::Standard),
                None => None,
            };
            scanner::full_scan(distro_override).map_err(Into::into)
        }

        Commands::Init { yes, force } => commands::init::run_init(yes, force).map_err(Into::into),

        Commands::Add { path, mode, from_trace } => {
            if let Some(trace_log) = from_trace {
                if let Ok(paths) = sandbox::tracer::parse_strace_log(&trace_log) {
                    for p in paths {
                        if let Err(e) = config::add_user_mount(p, mode.clone()) {
                            eprintln!("Skipped adding path: {}", e);
                        }
                    }
                } else {
                    eprintln!("Could not read trace log at {}", trace_log.display());
                }
                Ok(())
            } else if let Some(p) = path {
                config::add_user_mount(p, mode).map_err(Into::into)
            } else {
                Err(anyhow::anyhow!("A path is required unless --from-trace is used"))
            }
        }

        Commands::Remove { path } => config::remove_user_mount(path).map_err(Into::into),

        Commands::Edit {} => config::edit_user_config().map_err(Into::into),

        Commands::Set { net, gui, optional } => (|| -> anyhow::Result<()> {
            if let Some(n) = net {
                let n_str = match n {
                    crate::sandbox::network::NetworkMode::Disable => "disable",
                    crate::sandbox::network::NetworkMode::Allow => "allow",
                    crate::sandbox::network::NetworkMode::Full => "full",
                };
                config::set_profile_field(config::ProfileField::Network(n_str.to_string()))?;
            }
            if gui {
                config::set_profile_field(config::ProfileField::Gui(true))?;
            }
            if let Some(opt) = optional {
                config::set_profile_field(config::ProfileField::OptionalAdd(opt))?;
            }
            Ok(())
        })(),

        Commands::Unset { net, gui, optional } => (|| -> anyhow::Result<()> {
            if net {
                config::unset_profile_field(config::ProfileUnsetField::Network)?;
            }
            if gui {
                config::unset_profile_field(config::ProfileUnsetField::Gui)?;
            }
            if let Some(opt) = optional {
                config::unset_profile_field(config::ProfileUnsetField::OptionalRemove(opt))?;
            }
            Ok(())
        })(),

        Commands::Check => commands::check::run_check().map_err(Into::into),

        Commands::List => commands::list::run_list().map_err(Into::into),

        Commands::Status => commands::status::run_status().map_err(Into::into),

        Commands::Log { last, errors } => commands::log::run_log(last, errors).map_err(Into::into),

        Commands::Doctor => commands::doctor::run_doctor().map_err(Into::into),

        Commands::Profile { action } => match action {
            cli::ProfileCommands::Create { name, net, gui, optional } => {
                commands::profile::run_create(name, net, gui, optional)
            }
            cli::ProfileCommands::List => commands::profile::run_list(),
            cli::ProfileCommands::Delete { name } => commands::profile::run_delete(name),
            cli::ProfileCommands::Show { name } => commands::profile::run_show(name),
        }
        .map_err(Into::into),
    };

    if let Err(e) = result {
        // Check if it is a typed CordonError — if so, render a premium diagnostic box.
        if let Some(cordon_err) = e.downcast_ref::<CordonError>() {
            print_diagnostic_box(cordon_err);
            let exit_code = extract_exit_code_from_cordon(cordon_err);
            print_failure_reason(cordon_err, exit_code);

            // Use the structured exit code where possible.
            let code = match cordon_err {
                CordonError::ExecutionError(c) => *c,
                CordonError::CommandNotFound(_) => exit_codes::COMMAND_NOT_FOUND,
                CordonError::PermissionDenied(_) => exit_codes::COMMAND_NOT_EXECUTABLE,
                CordonError::DependencyMissing(_) => exit_codes::SANDBOX_SETUP_FAILED,
                CordonError::NamespaceError(_) => exit_codes::SANDBOX_SETUP_FAILED,
                _ => exit_codes::INTERNAL_ERROR,
            };
            std::process::exit(code);
        }

        // Fall back to plain error rendering for non-CordonError anyhow errors.
        eprintln!("error: {e:#}");
        std::process::exit(exit_codes::INTERNAL_ERROR);
    } else if !is_quiet {
        eprintln!("\x1b[90m[CORDON] sandbox exited cleanly — cage destroyed\x1b[0m");
    }
}

/// Renders an ASCII diagnostic box for Cordon failures — matching LION's style.
fn print_diagnostic_box(err: &CordonError) {
    eprintln!("\n+----------------------------------------------------------+");
    eprintln!("|  CORDON ERROR                                            |");
    eprintln!("+----------------------------------------------------------+");

    // Wrap each line of the error message neatly inside the box.
    let msg = format!("{}", err);
    for line in msg.lines() {
        eprintln!("| {:<56} |", line);
    }

    // Extra hint for permission-denied errors.
    if let CordonError::PermissionDenied(path) = err {
        eprintln!("+----------------------------------------------------------+");
        eprintln!("| Try: chmod +x {:<42} |", path);
    }

    eprintln!("+----------------------------------------------------------+");
    eprintln!("| See full log: ~/.config/cordon/logs/last-run.log         |");
    eprintln!("+----------------------------------------------------------+");
}

/// Prints a one-line human-readable hint after the diagnostic box.
fn print_failure_reason(err: &CordonError, exit_code: Option<i32>) {
    let reason: std::borrow::Cow<str> = match err {
        CordonError::CommandNotFound(_) => {
            "binary not found inside sandbox — check the command name".into()
        }
        CordonError::PermissionDenied(_) => "executable permission missing".into(),
        CordonError::DependencyMissing(dep) => {
            format!("'{}' is not installed — install it and retry", dep).into()
        }
        CordonError::NamespaceError(_) => {
            "bubblewrap cannot create user namespaces — run 'cordon scan' or check AppArmor".into()
        }
        CordonError::ScanRequired => "run 'cordon scan' to initialise sandbox configuration".into(),
        CordonError::ExecutionError(_) => match exit_code {
            // curl / wget: couldn't resolve host — almost always means no network.
            Some(6) => {
                "couldn't resolve host — sandbox network is disabled. Try: --net=allow or --net=full"
                    .into()
            }
            // curl: failed to connect.
            Some(7) => "connection refused or unreachable — try: --net=full".into(),
            // curl: SSL/TLS error.
            Some(35) => "SSL handshake failed inside sandbox — try: --net=full".into(),
            Some(code) => format!("command exited with code {code} (check program output above)").into(),
            None => "command failed inside sandbox".into(),
        },
        CordonError::Internal(_) => "internal sandbox setup failure".into(),
    };
    eprintln!("(reason: {})", reason);
}

/// Extracts the raw integer exit code from an `ExecutionError`.
fn extract_exit_code_from_cordon(err: &CordonError) -> Option<i32> {
    if let CordonError::ExecutionError(code) = err {
        Some(*code)
    } else {
        None
    }
}
