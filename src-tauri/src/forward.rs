use serde::Serialize;
use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::Mutex;

pub struct ForwardState(pub Mutex<HashMap<String, (Child, ForwardInfo)>>);

impl Default for ForwardState {
    fn default() -> Self {
        ForwardState(Mutex::new(HashMap::new()))
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct ForwardInfo {
    pub id: String,
    pub alias: String,
    pub direction: String, // "local" or "remote"
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

#[tauri::command]
pub fn start_forward(
    state: tauri::State<ForwardState>,
    alias: String,
    direction: String,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
) -> Result<ForwardInfo, String> {
    let flag = if direction == "remote" { "-R" } else { "-L" };
    let spec = format!("{local_port}:{remote_host}:{remote_port}");

    let child = Command::new("ssh")
        .arg("-N")
        .arg(flag)
        .arg(&spec)
        .arg(&alias)
        .spawn()
        .map_err(|e| format!("Failed to spawn ssh: {e}"))?;

    let id = uuid::Uuid::new_v4().to_string();
    let info = ForwardInfo {
        id: id.clone(),
        alias,
        direction,
        local_port,
        remote_host,
        remote_port,
    };
    state.0.lock().unwrap().insert(id, (child, info.clone()));
    Ok(info)
}

#[tauri::command]
pub fn stop_forward(state: tauri::State<ForwardState>, id: String) -> Result<(), String> {
    let mut map = state.0.lock().unwrap();
    if let Some((mut child, _)) = map.remove(&id) {
        let _ = child.kill();
        Ok(())
    } else {
        Err("Forward not found".into())
    }
}

#[tauri::command]
pub fn list_forwards(state: tauri::State<ForwardState>) -> Vec<ForwardInfo> {
    state.0.lock().unwrap().values().map(|(_, i)| i.clone()).collect()
}
