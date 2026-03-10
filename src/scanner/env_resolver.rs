/// Replaces the hardcoded `/run/user/1000` placeholder with the real
/// `$XDG_RUNTIME_DIR` value, which is user-specific (e.g. `/run/user/1001`).
///
/// Also replaces `$HOME` with the actual `HOME` environment variable.
///
/// Called once per module path at scan time. The resolved concrete path is
/// stored in system.toml so bwrap and integrity_check never read env vars.
pub fn resolve_env_vars(path: &str) -> String {
    let mut resolved_path = path.to_string();

    if resolved_path.contains("/run/user/1000")
        && let Ok(val) = std::env::var("XDG_RUNTIME_DIR") {
            resolved_path = resolved_path.replace("/run/user/1000", &val);
        }

    if resolved_path.contains("$HOME")
        && let Ok(val) = std::env::var("HOME") {
            resolved_path = resolved_path.replace("$HOME", &val);
        }

    resolved_path
}

/// Resolves the D-Bus session socket path at scan time.
///
/// Tries two sources in order:
///   1. `$DBUS_SESSION_BUS_ADDRESS` — e.g. `unix:path=/run/user/1000/bus`
///      Strips the `unix:path=` prefix and any `,guid=…` suffix.
///   2. `$XDG_RUNTIME_DIR/bus` — the conventional fallback location.
///
/// Returns `None` if the socket cannot be found on this system.
pub fn resolve_dbus_socket() -> Option<String> {
    // Primary: parse DBUS_SESSION_BUS_ADDRESS
    if let Ok(addr) = std::env::var("DBUS_SESSION_BUS_ADDRESS")
        && let Some(path_part) = addr.strip_prefix("unix:path=")
    {
        // Strip optional trailing ",guid=…" or other parameters
        let path = path_part.split(',').next().unwrap_or(path_part);
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    // Fallback: $XDG_RUNTIME_DIR/bus
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let path = format!("{}/bus", runtime_dir);
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }

    None
}

/// Resolves the PipeWire audio socket path at scan time.
///
/// Tries two sources in order:
///   1. `$PIPEWIRE_RUNTIME_DIR/pipewire-0`
///   2. `$XDG_RUNTIME_DIR/pipewire-0` — the conventional fallback location.
///
/// Returns `None` if the socket cannot be found on this system.
pub fn resolve_pipewire_socket() -> Option<String> {
    if let Ok(runtime_dir) = std::env::var("PIPEWIRE_RUNTIME_DIR") {
        let path = format!("{}/pipewire-0", runtime_dir);
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }

    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let path = format!("{}/pipewire-0", runtime_dir);
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }

    None
}

/// Resolves the PulseAudio legacy audio socket path at scan time.
///
/// Tries two sources in order:
///   1. `$PULSE_RUNTIME_PATH/native`
///   2. `$XDG_RUNTIME_DIR/pulse/native` — the conventional fallback location.
///
/// Returns `None` if the socket cannot be found on this system.
pub fn resolve_pulse_socket() -> Option<String> {
    if let Ok(runtime_dir) = std::env::var("PULSE_RUNTIME_PATH") {
        let path = format!("{}/native", runtime_dir);
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }

    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let path = format!("{}/pulse/native", runtime_dir);
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }

    None
}
