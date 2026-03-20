//! suggestions.rs
//!
//! Smart error handling for Cordon:
//!   - Unknown subcommand   → Levenshtein nearest-match → "did you mean cordon <cmd>?"
//!   - Missing required arg → Show full command syntax
//!
//! All known commands are listed in KNOWN_COMMANDS. Adding a new subcommand to
//! cli.rs means adding one entry here — that's the only maintenance required.

/// Every currently implemented subcommand name.
/// Keep this in sync with the `Commands` enum in cli.rs.
pub const KNOWN_COMMANDS: &[&str] = &[
    "run",
    "scan",
    "add",
    "remove",
    "check",
    "list",
    "status",
    "profile",
];

/// Per-command usage synopsis printed when a required argument is missing.
/// Key = subcommand name (matches KNOWN_COMMANDS entry exactly).
pub fn command_synopsis(cmd: &str) -> Option<&'static str> {
    match cmd {
        "run" => Some(
            "cordon run [--net <disable|allow|full>] [--domain <DOMAIN>]... \
             [--gui] [--optional <MODULE>]... [--dry-run] [--debug] -- <cmd> [args...]"
        ),
        "scan"   => Some("cordon scan"),
        "add"    => Some("cordon add <path> [--mode <ro|rw>]"),
        "remove" => Some("cordon remove <path>"),
        "edit"   => Some("cordon edit"),
        "check"  => Some("cordon check"),
        "list"   => Some("cordon list"),
        "status" => Some("cordon status"),
        "profile" => Some("cordon profile <create|list|delete|show>"),
        _        => None,
    }
}

/// Compute the Levenshtein distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (na, nb) = (a.len(), b.len());

    // dp[j] = edit distance between a[0..i] and b[0..j]
    let mut dp: Vec<usize> = (0..=nb).collect();

    for i in 1..=na {
        let mut prev = dp[0];
        dp[0] = i;
        for j in 1..=nb {
            let old = dp[j];
            dp[j] = if a[i - 1] == b[j - 1] {
                prev
            } else {
                1 + prev.min(dp[j]).min(dp[j - 1])
            };
            prev = old;
        }
    }
    dp[nb]
}

/// Find the closest known command to `input`.
/// Returns `None` if no command is within a reasonable distance (≤ 3 edits)
/// or if `input` is empty.
pub fn closest_command(input: &str) -> Option<&'static str> {
    if input.is_empty() {
        return None;
    }
    let input_lower = input.to_lowercase();
    KNOWN_COMMANDS
        .iter()
        .map(|&cmd| (cmd, levenshtein(&input_lower, cmd)))
        .filter(|(_, dist)| *dist <= 3)
        .min_by_key(|(_, dist)| *dist)
        .map(|(cmd, _)| cmd)
}

/// Print a formatted "unknown command" error with a "did you mean?" suggestion
/// and optionally show the full syntax of the suggested command.
///
/// Called from main.rs when clap returns an `InvalidSubcommand` error.
pub fn print_unknown_command_error(bad_cmd: &str) {
    eprintln!(
        "\n\x1b[1;31merror:\x1b[0m \x1b[1mcordon {bad_cmd}\x1b[0m — unknown subcommand"
    );

    match closest_command(bad_cmd) {
        Some(best) => {
            eprintln!(
                "\n  \x1b[90mDid you mean?\x1b[0m  \x1b[1;96mcordon {best}\x1b[0m"
            );
            if let Some(syn) = command_synopsis(best) {
                eprintln!("\n  \x1b[90mUsage:\x1b[0m  {syn}");
            }
        }
        None => {
            eprintln!("\n  No similar command found.");
            eprintln!(
                "  Run \x1b[1mcordon --help\x1b[0m to see all available commands."
            );
        }
    }

    eprintln!();
}

/// Print a "missing required argument" error with the full command syntax.
///
/// `subcommand` is the name of the subcommand the user was trying to use
/// (extracted from the clap error context). May be `None` if we can't determine it.
pub fn print_missing_arg_error(missing_arg: &str, subcommand: Option<&str>) {
    eprintln!(
        "\n\x1b[1;31merror:\x1b[0m missing required argument: \x1b[1m{missing_arg}\x1b[0m"
    );

    if let Some(sub) = subcommand {
        if let Some(syn) = command_synopsis(sub) {
            eprintln!("\n  \x1b[90mUsage:\x1b[0m  {syn}");
        }
    }

    eprintln!(
        "\n  Run \x1b[1mcordon {} --help\x1b[0m for full details.\n",
        subcommand.unwrap_or("<command>")
    );
}
