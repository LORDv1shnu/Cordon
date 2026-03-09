/// Resolves `/run/user/1000` placeholder to the real `$XDG_RUNTIME_DIR`.
///
/// Called ONCE at scan time. The resolved concrete path is stored in
/// system.toml so the quick check and bwrap never need to touch env vars.
#[allow(clippy::collapsible_if)]
pub fn resolve_env_vars(path: &str) -> String {
    if let Ok(val) = std::env::var("XDG_RUNTIME_DIR") {
        if path.contains("/run/user/1000") {
            return path.replace("/run/user/1000", &val);
        }
    }
    path.to_string()
}
