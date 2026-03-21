#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Distro {
    NixOS,
    Standard,
}

pub fn detect_distro() -> Distro {
    if std::path::Path::new("/etc/os-release").exists() {
        if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
            if content.contains("ID=nixos") || content.contains("ID=\"nixos\"") {
                return Distro::NixOS;
            }
        }
    }
    Distro::Standard
}
