use crate::ssh_config::{parse_ssh_config, SshHost};
use serde::Serialize;
use ssh2::Session;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;

#[derive(Serialize, Clone, Debug)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
}

fn resolve_host(alias: &str) -> Result<SshHost, String> {
    parse_ssh_config()
        .into_iter()
        .find(|h| h.alias == alias)
        .ok_or_else(|| format!("No entry for alias '{alias}' in ~/.ssh/config"))
}

fn connect(alias: &str) -> Result<Session, String> {
    let host = resolve_host(alias)?;
    let hostname = host.host_name.unwrap_or_else(|| alias.to_string());
    let port: u16 = host.port.and_then(|p| p.parse().ok()).unwrap_or(22);
    let user = host
        .user
        .or_else(|| std::env::var("USER").ok())
        .ok_or("Could not determine SSH user")?;

    let tcp = TcpStream::connect((hostname.as_str(), port))
        .map_err(|e| format!("TCP connect to {hostname}:{port} failed: {e}"))?;
    let mut sess = Session::new().map_err(|e| e.to_string())?;
    sess.set_tcp_stream(tcp);
    sess.handshake().map_err(|e| format!("SSH handshake failed: {e}"))?;

    // Try ssh-agent first (matches ForwardAgent-based workflows), then identity file.
    let mut authed = false;
    if let Ok(mut agent) = sess.agent() {
        if agent.connect().is_ok() && agent.list_identities().is_ok() {
            if let Ok(identities) = agent.identities() {
                for id in identities {
                    if agent.userauth(&user, &id).is_ok() {
                        authed = true;
                        break;
                    }
                }
            }
        }
    }
    if !authed {
        let key_path = host
            .identity_file
            .map(|p| shellexpand::tilde(&p).to_string())
            .unwrap_or_else(|| shellexpand::tilde("~/.ssh/id_ed25519").to_string());
        sess.userauth_pubkey_file(&user, None, Path::new(&key_path), None)
            .map_err(|e| format!("Public key auth failed with {key_path}: {e}"))?;
        authed = sess.authenticated();
    }
    if !authed {
        return Err(
            "PERMISSION_DENIED: SSH authentication failed. No working ssh-agent identity or key was accepted for this host.".into(),
        );
    }
    Ok(sess)
}

#[derive(Serialize, Clone, Debug)]
pub struct AgentStatus {
    pub agent_running: bool,
    pub identity_count: usize,
}

/// Lets the frontend proactively check whether ssh-agent has any keys loaded,
/// so it can prompt the user to grant access (via `ssh-add`) before a real
/// connection attempt fails with a permission error.
#[tauri::command]
pub fn check_ssh_agent() -> AgentStatus {
    let sess = match Session::new() {
        Ok(s) => s,
        Err(_) => return AgentStatus { agent_running: false, identity_count: 0 },
    };
    match sess.agent() {
        Ok(mut agent) => {
            if agent.connect().is_err() {
                return AgentStatus { agent_running: false, identity_count: 0 };
            }
            let count = agent
                .list_identities()
                .ok()
                .and_then(|_| agent.identities().ok())
                .map(|ids| ids.len())
                .unwrap_or(0);
            AgentStatus { agent_running: true, identity_count: count }
        }
        Err(_) => AgentStatus { agent_running: false, identity_count: 0 },
    }
}

#[tauri::command]
pub fn list_dir(alias: String, path: String) -> Result<Vec<RemoteEntry>, String> {
    let sess = connect(&alias)?;
    let sftp = sess.sftp().map_err(|e| e.to_string())?;
    let entries = sftp.readdir(Path::new(&path)).map_err(|e| e.to_string())?;
    let mut out = vec![];
    for (p, stat) in entries {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        out.push(RemoteEntry {
            name,
            path: p.to_string_lossy().to_string(),
            is_dir: stat.is_dir(),
            size: stat.size.unwrap_or(0),
            modified: stat.mtime.unwrap_or(0),
        });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(out)
}

#[tauri::command]
pub fn download_file(alias: String, remote_path: String, local_path: String) -> Result<String, String> {
    let sess = connect(&alias)?;
    let sftp = sess.sftp().map_err(|e| e.to_string())?;
    let mut remote_file = sftp
        .open(Path::new(&remote_path))
        .map_err(|e| format!("Failed to open remote file: {e}"))?;
    let mut buf = Vec::new();
    remote_file
        .read_to_end(&mut buf)
        .map_err(|e| format!("Failed to read remote file: {e}"))?;
    let mut local_file =
        std::fs::File::create(&local_path).map_err(|e| format!("Failed to create local file: {e}"))?;
    local_file
        .write_all(&buf)
        .map_err(|e| format!("Failed to write local file: {e}"))?;
    Ok(local_path)
}
