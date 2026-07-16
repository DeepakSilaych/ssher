use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;

#[derive(Serialize, Clone, Debug, Default)]
pub struct SshHost {
    pub alias: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<String>,
    pub identity_file: Option<String>,
}

/// Minimal ~/.ssh/config parser: handles `Host` blocks and the directives
/// we care about (HostName, User, Port, IdentityFile). Wildcard-only hosts
/// (containing `*`/`?`) are skipped since they aren't real connectable aliases.
pub fn parse_ssh_config() -> Vec<SshHost> {
    let path = match dirs::home_dir() {
        Some(h) => h.join(".ssh").join("config"),
        None => return vec![],
    };
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let mut hosts: Vec<SshHost> = vec![];
    let mut current_aliases: Vec<String> = vec![];
    let mut current_fields: HashMap<String, String> = HashMap::new();

    let flush = |aliases: &Vec<String>, fields: &HashMap<String, String>, hosts: &mut Vec<SshHost>| {
        for alias in aliases {
            if alias.contains('*') || alias.contains('?') {
                continue;
            }
            hosts.push(SshHost {
                alias: alias.clone(),
                host_name: fields.get("hostname").cloned(),
                user: fields.get("user").cloned(),
                port: fields.get("port").cloned(),
                identity_file: fields.get("identityfile").cloned(),
            });
        }
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let key = parts.next().unwrap_or("").to_lowercase();
        let value = parts.next().unwrap_or("").trim().trim_matches('"').to_string();

        if key == "host" {
            flush(&current_aliases, &current_fields, &mut hosts);
            current_aliases = value.split_whitespace().map(|s| s.to_string()).collect();
            current_fields = HashMap::new();
        } else if !current_aliases.is_empty() {
            current_fields.entry(key).or_insert(value);
        }
    }
    flush(&current_aliases, &current_fields, &mut hosts);

    hosts
}

#[derive(Deserialize, Debug)]
pub struct NewSshHost {
    pub alias: String,
    pub host_name: String,
    pub user: Option<String>,
    pub port: Option<String>,
    pub identity_file: Option<String>,
}

#[tauri::command]
pub fn add_ssh_host(host: NewSshHost) -> Result<(), String> {
    let ssh_dir = dirs::home_dir()
        .map(|h| h.join(".ssh"))
        .ok_or("Could not determine home directory")?;
    fs::create_dir_all(&ssh_dir).map_err(|e| format!("Failed to create ~/.ssh: {e}"))?;
    let path = ssh_dir.join("config");

    let alias = host.alias.trim();
    if alias.is_empty() {
        return Err("Alias cannot be empty".into());
    }
    if parse_ssh_config().iter().any(|h| h.alias == alias) {
        return Err(format!("Host alias '{alias}' already exists in ~/.ssh/config"));
    }

    let mut block = format!("\nHost {alias}\n  HostName {}\n", host.host_name.trim());
    if let Some(u) = host.user.filter(|s| !s.trim().is_empty()) {
        block.push_str(&format!("  User {}\n", u.trim()));
    }
    if let Some(p) = host.port.filter(|s| !s.trim().is_empty()) {
        block.push_str(&format!("  Port {}\n", p.trim()));
    }
    if let Some(i) = host.identity_file.filter(|s| !s.trim().is_empty()) {
        block.push_str(&format!("  IdentityFile {}\n", i.trim()));
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Failed to open {}: {e}", path.display()))?;
    file.write_all(block.as_bytes())
        .map_err(|e| format!("Failed to write to {}: {e}", path.display()))?;
    Ok(())
}
