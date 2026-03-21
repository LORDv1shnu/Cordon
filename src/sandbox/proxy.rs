//! src/sandbox/proxy.rs
//!
//! Native Rust implementation of the domain-filtering HTTP/HTTPS proxy.
//! Ported from lion's Python proxy logic.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use serde::Deserialize;

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ProxyConfig {
    #[serde(default)]
    pub domains: Vec<String>,
}

/// Built-in default allow-list.
const DEFAULT_DOMAINS: &[&str] = &[
    "registry.npmjs.org", "npmjs.org", "nodejs.org",
    "pypi.org", "files.pythonhosted.org", "bootstrap.pypa.io",
    "crates.io", "static.crates.io", "index.crates.io",
    "github.com", "api.github.com", "raw.githubusercontent.com",
    "objects.githubusercontent.com", "codeload.github.com",
    "google.com", "www.google.com",
];

pub fn load_config(project_dir: &Path) -> ProxyConfig {
    let local = project_dir.join("proxy.toml");
    if local.exists() {
        return load_from_path(&local, "local");
    }

    if let Ok(home) = std::env::var("HOME") {
        let global = std::path::PathBuf::from(home).join(".config/cordon/proxy.toml");
        if global.exists() {
            return load_from_path(&global, "global");
        }
    }

    ProxyConfig {
        domains: DEFAULT_DOMAINS.iter().map(|s| s.to_string()).collect(),
    }
}

fn load_from_path(path: &Path, _label: &str) -> ProxyConfig {
    match std::fs::read_to_string(path) {
        Ok(contents) => toml::from_str::<ProxyConfig>(&contents).unwrap_or_default(),
        Err(_) => ProxyConfig::default(),
    }
}

pub struct ProxyHandle {
    pub port: u16,
    // Note: Rust doesn't have an easy way to 'kill' a thread. 
    // In this simple implementation, we let the threads die when the process exits
    // or when the listener is dropped (though TcpListener doesn't stop ongoing threads).
    // For Cordon, this is sufficient as the whole process exits after the sandbox.
}

impl ProxyHandle {
    pub fn spawn(allowed_domains: Vec<String>) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        
        let domains = Arc::new(if allowed_domains.contains(&"*".to_string()) {
            None
        } else {
            Some(allowed_domains.into_iter().map(|d| d.to_lowercase()).collect::<Vec<_>>())
        });

        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let domains_clone = Arc::clone(&domains);
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, domains_clone) {
                        // Silently ignore connection errors for now
                        let _ = e;
                    }
                });
            }
        });

        Ok(ProxyHandle { port })
    }
}

fn is_allowed(target: &str, allowed: &Arc<Option<Vec<String>>>) -> bool {
    let allowed_list = match allowed.as_ref() {
        Some(list) => list,
        None => return true, // "*" was present
    };

    let host = target.split(':').next().unwrap_or(target).to_lowercase();
    
    if allowed_list.contains(&host) {
        return true;
    }

    for d in allowed_list {
        if host.ends_with(&format!(".{}", d)) {
            return true;
        }
    }

    false
}

fn handle_connection(mut client: TcpStream, allowed: Arc<Option<Vec<String>>>) -> std::io::Result<()> {
    let mut buffer = [0; 8192];
    let n = client.read(&mut buffer)?;
    if n == 0 { return Ok(()); }

    let request = String::from_utf8_lossy(&buffer[..n]);
    let first_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    
    if parts.len() < 2 { return Ok(()); }
    let method = parts[0];
    let target = parts[1];

    if method == "CONNECT" {
        if is_allowed(target, &allowed) {
            log_allow(target);
            let mut server = TcpStream::connect(target)?;
            client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")?;
            
            let mut client_clone = client.try_clone()?;
            let mut server_clone = server.try_clone()?;

            thread::spawn(move || {
                let _ = std::io::copy(&mut client_clone, &mut server_clone);
            });
            let _ = std::io::copy(&mut server, &mut client);
        } else {
            log_block(target);
            client.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")?;
        }
    } else {
        // Plain HTTP
        let mut host = "";
        for line in request.lines() {
            if line.to_lowercase().starts_with("host:") {
                host = line[5..].trim();
                break;
            }
        }

        let domain = if host.is_empty() { target } else { host };

        if is_allowed(domain, &allowed) {
            log_allow(domain);
            let target_addr = if domain.contains(':') {
                domain.to_string()
            } else {
                format!("{}:80", domain)
            };
            let mut server = TcpStream::connect(target_addr)?;
            server.write_all(&buffer[..n])?;
            
            let mut client_clone = client.try_clone()?;
            let mut server_clone = server.try_clone()?;

            thread::spawn(move || {
                let _ = std::io::copy(&mut server_clone, &mut client_clone);
            });
            let _ = std::io::copy(&mut client, &mut server);
        } else {
            log_block(domain);
            client.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")?;
        }
    }

    Ok(())
}

fn log_allow(domain: &str) {
    println!("\x1b[90m[CORDON-PROXY]\x1b[0m \x1b[1;32mALLOWED\x1b[0m {}", domain);
}

fn log_block(domain: &str) {
    println!("\x1b[90m[CORDON-PROXY]\x1b[0m \x1b[1;31mBLOCKED\x1b[0m {}", domain);
}
