use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const MAX_BACKOFF_SECS: u64 = 30;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ForwardConfig {
    pub id: String,
    pub alias: String,
    pub direction: String, // "local" or "remote"
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ForwardStatus {
    Connecting,
    Active,
    Retrying,
    Stopped,
}

#[derive(Serialize, Clone, Debug)]
pub struct ForwardInfo {
    #[serde(flatten)]
    pub config: ForwardConfig,
    pub status: ForwardStatus,
    pub last_error: Option<String>,
    pub retry_count: u32,
}

struct ForwardEntry {
    config: ForwardConfig,
    status: ForwardStatus,
    last_error: Option<String>,
    retry_count: u32,
    stop_flag: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

impl ForwardEntry {
    fn info(&self) -> ForwardInfo {
        ForwardInfo {
            config: self.config.clone(),
            status: self.status.clone(),
            last_error: self.last_error.clone(),
            retry_count: self.retry_count,
        }
    }
}

type ForwardMap = Arc<Mutex<HashMap<String, ForwardEntry>>>;

pub struct ForwardState(ForwardMap);

impl Default for ForwardState {
    fn default() -> Self {
        ForwardState(Arc::new(Mutex::new(HashMap::new())))
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ssher").join("forwards.json"))
}

fn persist(map: &ForwardMap) {
    let Some(path) = config_path() else { return };
    let configs: Vec<ForwardConfig> = map
        .lock()
        .unwrap()
        .values()
        .map(|e| e.config.clone())
        .collect();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&configs) {
        let _ = fs::write(&path, json);
    }
}

pub fn load_persisted() -> Vec<ForwardConfig> {
    let Some(path) = config_path() else {
        return vec![];
    };
    let Ok(content) = fs::read_to_string(&path) else {
        return vec![];
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn emit_status(app: &AppHandle, map: &ForwardMap, id: &str) {
    let info = {
        let guard = map.lock().unwrap();
        guard.get(id).map(|e| e.info())
    };
    if let Some(info) = info {
        let _ = app.emit("forward-status", info);
    }
}

/// Spawns a background thread that keeps a single forward alive: (re)connects
/// on failure/drop with exponential backoff, until explicitly stopped.
fn supervise(app: AppHandle, map: ForwardMap, id: String) {
    thread::spawn(move || {
        let mut backoff = 1u64;
        loop {
            let stop_flag = {
                let guard = map.lock().unwrap();
                match guard.get(&id) {
                    Some(e) => e.stop_flag.clone(),
                    None => return,
                }
            };
            if stop_flag.load(Ordering::SeqCst) {
                return;
            }

            let config = {
                let mut guard = map.lock().unwrap();
                let Some(e) = guard.get_mut(&id) else { return };
                e.status = ForwardStatus::Connecting;
                e.config.clone()
            };
            emit_status(&app, &map, &id);

            let flag = if config.direction == "remote" { "-R" } else { "-L" };
            let spec = format!("{}:{}:{}", config.local_port, config.remote_host, config.remote_port);

            // Notes on ssh options, learned the hard way:
            // - ControlMaster=no / ControlPath=none: with a shared master active, ssh
            //   would hand the forward off to it and exit immediately, making a healthy
            //   tunnel look like a crash loop.
            // - NO ExitOnForwardFailure: the host's ssh_config may carry unrelated static
            //   forwards (e.g. a RemoteForward held by another session); with that option
            //   their collision kills the whole connection even though OUR forward is fine.
            //   Instead we verify our own forward with a real TCP probe below.
            // - NO ClearAllForwardings: it clears command-line -L/-R too, silently leaving
            //   a connection with no forward at all.
            let spawn_result = Command::new("ssh")
                .arg("-N")
                .arg("-o")
                .arg("ServerAliveInterval=10")
                .arg("-o")
                .arg("ServerAliveCountMax=3")
                .arg("-o")
                .arg("ControlMaster=no")
                .arg("-o")
                .arg("ControlPath=none")
                .arg(flag)
                .arg(&spec)
                .arg(&config.alias)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn();

            match spawn_result {
                Ok(child) => {
                    let connected_at = std::time::Instant::now();
                    let child_arc = {
                        let mut guard = map.lock().unwrap();
                        let Some(e) = guard.get_mut(&id) else {
                            let mut c = child;
                            let _ = c.kill();
                            return;
                        };
                        *e.child.lock().unwrap() = Some(child);
                        e.child.clone()
                    };

                    // Watch the tunnel: process must stay alive, and for local forwards
                    // the local port must actually accept connections. Only then is it
                    // reported Active. On any failure, kill the process and retry.
                    let is_local = config.direction != "remote";
                    let probe_addr: SocketAddr =
                        format!("127.0.0.1:{}", config.local_port).parse().unwrap();
                    let mut reported_active = false;
                    let failure: Option<String> = loop {
                        if stop_flag.load(Ordering::SeqCst) {
                            break None;
                        }
                        let exited = {
                            let mut guard = child_arc.lock().unwrap();
                            match guard.as_mut() {
                                Some(c) => c.try_wait().ok().flatten(),
                                None => break None,
                            }
                        };
                        if let Some(status) = exited {
                            break Some(format!("ssh exited unexpectedly ({status})"));
                        }

                        let healthy = if is_local {
                            TcpStream::connect_timeout(&probe_addr, Duration::from_secs(2)).is_ok()
                        } else {
                            true
                        };

                        if healthy {
                            if !reported_active {
                                reported_active = true;
                                backoff = 1;
                                let mut guard = map.lock().unwrap();
                                if let Some(e) = guard.get_mut(&id) {
                                    e.status = ForwardStatus::Active;
                                    e.last_error = None;
                                }
                                drop(guard);
                                emit_status(&app, &map, &id);
                            }
                        } else if connected_at.elapsed() > Duration::from_secs(10) || reported_active {
                            break Some(format!(
                                "local port {} is not accepting connections",
                                config.local_port
                            ));
                        }

                        thread::sleep(Duration::from_secs(3));
                    };

                    if let Some(mut c) = child_arc.lock().unwrap().take() {
                        let _ = c.kill();
                        let _ = c.wait();
                    }

                    if stop_flag.load(Ordering::SeqCst) || failure.is_none() {
                        let mut guard = map.lock().unwrap();
                        if let Some(e) = guard.get_mut(&id) {
                            e.status = ForwardStatus::Stopped;
                        }
                        drop(guard);
                        emit_status(&app, &map, &id);
                        return;
                    }

                    let mut guard = map.lock().unwrap();
                    let Some(e) = guard.get_mut(&id) else { return };
                    e.retry_count += 1;
                    e.last_error = failure;
                    e.status = ForwardStatus::Retrying;
                    drop(guard);
                    emit_status(&app, &map, &id);
                }
                Err(e) => {
                    if stop_flag.load(Ordering::SeqCst) {
                        return;
                    }
                    let mut guard = map.lock().unwrap();
                    let Some(entry) = guard.get_mut(&id) else { return };
                    entry.retry_count += 1;
                    entry.last_error = Some(format!("Failed to spawn ssh: {e}"));
                    entry.status = ForwardStatus::Retrying;
                    drop(guard);
                    emit_status(&app, &map, &id);
                }
            }

            thread::sleep(Duration::from_secs(backoff));
            backoff = (backoff * 2).min(MAX_BACKOFF_SECS);
        }
    });
}

fn insert_and_start(app: &AppHandle, map: &ForwardMap, config: ForwardConfig) {
    let id = config.id.clone();
    let entry = ForwardEntry {
        config,
        status: ForwardStatus::Connecting,
        last_error: None,
        retry_count: 0,
        stop_flag: Arc::new(AtomicBool::new(false)),
        child: Arc::new(Mutex::new(None)),
    };
    map.lock().unwrap().insert(id.clone(), entry);
    supervise(app.clone(), map.clone(), id);
}

/// Restores forwards saved from a previous run and reconnects them automatically.
pub fn restore_on_startup(app: &AppHandle, state: &ForwardState) {
    for config in load_persisted() {
        insert_and_start(app, &state.0, config);
    }
}

#[tauri::command]
pub fn start_forward(
    app: AppHandle,
    state: tauri::State<ForwardState>,
    alias: String,
    direction: String,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
) -> Result<ForwardInfo, String> {
    let config = ForwardConfig {
        id: uuid::Uuid::new_v4().to_string(),
        alias,
        direction,
        local_port,
        remote_host,
        remote_port,
    };
    let info = ForwardInfo {
        config: config.clone(),
        status: ForwardStatus::Connecting,
        last_error: None,
        retry_count: 0,
    };
    insert_and_start(&app, &state.0, config);
    persist(&state.0);
    Ok(info)
}

/// Stops a forward for good: kills the running process (if any) and removes
/// its persisted config so it won't be reconnected on the next app launch.
#[tauri::command]
pub fn stop_forward(state: tauri::State<ForwardState>, id: String) -> Result<(), String> {
    let entry = {
        let mut guard = state.0.lock().unwrap();
        guard.remove(&id)
    };
    match entry {
        Some(entry) => {
            entry.stop_flag.store(true, Ordering::SeqCst);
            if let Some(mut child) = entry.child.lock().unwrap().take() {
                let _ = child.kill();
            }
            persist(&state.0);
            Ok(())
        }
        None => Err("Forward not found".into()),
    }
}

#[tauri::command]
pub fn list_forwards(state: tauri::State<ForwardState>) -> Vec<ForwardInfo> {
    state.0.lock().unwrap().values().map(|e| e.info()).collect()
}
