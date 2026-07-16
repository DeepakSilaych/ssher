use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter};

/// direction: "push" (local -> remote) or "pull" (remote -> local)
#[tauri::command]
pub fn sync_folder(
    app: AppHandle,
    alias: String,
    local_path: String,
    remote_path: String,
    direction: String,
    delete: bool,
) -> Result<(), String> {
    let local_arg = if local_path.ends_with('/') {
        local_path.clone()
    } else {
        format!("{local_path}/")
    };
    let remote_arg = format!("{alias}:{remote_path}");
    let (src, dst) = if direction == "pull" {
        (remote_arg, local_arg)
    } else {
        (local_arg, remote_arg)
    };

    let mut args = vec!["-avz", "--progress"];
    if delete {
        args.push("--delete");
    }
    args.push(&src);
    args.push(&dst);

    let mut child = Command::new("rsync")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn rsync: {e}"))?;

    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take().ok_or("no stderr")?;

    let app_out = app.clone();
    let out_handle = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().flatten() {
            let _ = app_out.emit("sync-log", line);
        }
    });
    let app_err = app.clone();
    let err_handle = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().flatten() {
            let _ = app_err.emit("sync-log", format!("[stderr] {line}"));
        }
    });

    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = out_handle.join();
    let _ = err_handle.join();

    let _ = app.emit("sync-done", status.success());
    if status.success() {
        Ok(())
    } else {
        Err(format!("rsync exited with status {status}"))
    }
}
