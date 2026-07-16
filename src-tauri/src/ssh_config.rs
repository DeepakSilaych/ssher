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
    pub proxy_command: Option<String>,
}

struct Block {
    patterns: Vec<String>,
    fields: HashMap<String, String>,
}

/// Simple glob matcher supporting `*` and `?`, as used by ssh_config `Host` patterns.
fn glob_match(pattern: &str, text: &str) -> bool {
    fn helper(p: &[char], t: &[char]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some('*') => (0..=t.len()).any(|i| helper(&p[1..], &t[i..])),
            Some('?') => !t.is_empty() && helper(&p[1..], &t[1..]),
            Some(c) => t.first() == Some(c) && helper(&p[1..], &t[1..]),
        }
    }
    let pc: Vec<char> = pattern.chars().collect();
    let tc: Vec<char> = text.chars().collect();
    helper(&pc, &tc)
}

/// Minimal ~/.ssh/config parser: handles `Host` blocks and the directives we
/// care about (HostName, User, Port, IdentityFile, ProxyCommand).
///
/// Real ssh_config semantics: a target host can match *multiple* `Host`
/// blocks (including wildcard ones like `Host *`), and for each directive the
/// **first** matching block's value wins. We replicate that instead of
/// treating each `Host` line as an independent, disconnected host entry —
/// otherwise an alias whose settings are split across more than one block
/// (a common pattern: a shared block for defaults + a specific block for
/// per-host overrides) shows up as duplicate, incomplete entries.
pub fn parse_ssh_config() -> Vec<SshHost> {
    let path = match dirs::home_dir() {
        Some(h) => h.join(".ssh").join("config"),
        None => return vec![],
    };
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    parse_ssh_config_str(&content)
}

fn parse_ssh_config_str(content: &str) -> Vec<SshHost> {
    let mut blocks: Vec<Block> = vec![];
    let mut current: Option<Block> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let key = parts.next().unwrap_or("").to_lowercase();
        let value = parts.next().unwrap_or("").trim().trim_matches('"').to_string();

        if key == "host" {
            if let Some(b) = current.take() {
                blocks.push(b);
            }
            current = Some(Block {
                patterns: value.split_whitespace().map(String::from).collect(),
                fields: HashMap::new(),
            });
        } else if let Some(b) = current.as_mut() {
            b.fields.entry(key).or_insert(value);
        }
    }
    if let Some(b) = current.take() {
        blocks.push(b);
    }

    // Concrete (non-wildcard) aliases, in first-appearance order.
    let mut aliases: Vec<String> = vec![];
    for b in &blocks {
        for p in &b.patterns {
            if p.contains('*') || p.contains('?') {
                continue;
            }
            if !aliases.contains(p) {
                aliases.push(p.clone());
            }
        }
    }

    aliases
        .into_iter()
        .map(|alias| {
            let mut fields: HashMap<String, String> = HashMap::new();
            for b in &blocks {
                if b.patterns.iter().any(|p| glob_match(p, &alias)) {
                    for (k, v) in &b.fields {
                        fields.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                }
            }
            SshHost {
                alias: alias.clone(),
                host_name: fields.get("hostname").cloned(),
                user: fields.get("user").cloned(),
                port: fields.get("port").cloned(),
                identity_file: fields.get("identityfile").cloned(),
                proxy_command: fields.get("proxycommand").cloned(),
            }
        })
        .collect()
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

#[cfg(test)]
mod tests {
    #[test]
    fn merges_fields_across_matching_blocks() {
        let hosts = super::parse_ssh_config_str(
            "Host a b\n  User shared\n  IdentityFile ~/.ssh/id\n\nHost a\n  HostName 1.2.3.4\n",
        );
        assert_eq!(hosts.len(), 2);
        let a = hosts.iter().find(|h| h.alias == "a").unwrap();
        assert_eq!(a.host_name.as_deref(), Some("1.2.3.4"));
        assert_eq!(a.user.as_deref(), Some("shared"));
        let b = hosts.iter().find(|h| h.alias == "b").unwrap();
        assert_eq!(b.host_name, None);
        assert_eq!(b.user.as_deref(), Some("shared"));
    }

    #[test]
    fn skips_wildcard_only_patterns() {
        let hosts = super::parse_ssh_config_str("Host *\n  ServerAliveInterval 30\n");
        assert!(hosts.is_empty());
    }
}
