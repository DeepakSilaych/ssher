use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

/// rsync -i output prefixes each processed entry with an update code, e.g.
/// ">f+++++++++ path/to/file". The 2nd char is the item type: f=file, d=dir,
/// L=symlink, D=device, S=special — we only care about regular files.
///
/// The code's width isn't reliably 11 chars: GNU rsync pads it to 11, but
/// macOS ships `openrsync` (a BSD rsync clone), which emits a shorter,
/// variable-length code (e.g. ">f+++++++" — 9 chars) for the exact same
/// event. Split on the first space instead of assuming a fixed offset so
/// both implementations parse correctly.
fn parse_itemized_file(line: &str) -> Option<&str> {
    let (code, rest) = line.split_once(' ')?;
    let chars: Vec<char> = code.chars().collect();
    if chars.len() < 2 || rest.is_empty() {
        return None;
    }
    let is_update_code = matches!(chars[0], '<' | '>' | 'c' | 'h' | '.' | '*');
    if !is_update_code || chars[1] != 'f' {
        return None;
    }
    Some(rest)
}

/// direction: "push" (local -> remote) or "pull" (remote -> local)
#[tauri::command]
pub fn sync_folder(
    app: AppHandle,
    alias: String,
    local_path: String,
    remote_path: String,
    direction: String,
    delete: bool,
    copy_paths_to_clipboard: bool,
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

    let mut args = vec!["-avz", "--itemize-changes", "--progress"];
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

    let changed_files: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));

    let app_out = app.clone();
    let changed_files_out = changed_files.clone();
    let out_handle = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().flatten() {
            if let Some(path) = parse_itemized_file(&line) {
                changed_files_out.lock().unwrap().push(path.to_string());
            }
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
    if !status.success() {
        return Err(format!("rsync exited with status {status}"));
    }

    if copy_paths_to_clipboard {
        let files = changed_files.lock().unwrap();
        if files.is_empty() {
            let _ = app.emit("sync-log", "(no changed files to copy — nothing new to sync)".to_string());
        } else {
            let base = local_path.trim_end_matches('/');
            let abs_paths: Vec<String> = files.iter().map(|f| format!("{base}/{f}")).collect();
            match copy_to_clipboard(&abs_paths.join("\n")) {
                Ok(()) => {
                    let _ = app.emit(
                        "sync-log",
                        format!("Copied {} file path(s) to clipboard", abs_paths.len()),
                    );
                }
                Err(e) => {
                    let _ = app.emit("sync-log", format!("Failed to copy paths to clipboard: {e}"));
                }
            }
        }
    }

    Ok(())
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn pbcopy: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(text.as_bytes())
        .map_err(|e| format!("Failed to write to pbcopy: {e}"))?;
    child.wait().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_itemized_file;

    #[test]
    fn extracts_regular_file_paths_gnu_rsync_format() {
        assert_eq!(parse_itemized_file(">f+++++++++ dir/new.txt"), Some("dir/new.txt"));
        assert_eq!(parse_itemized_file(">f.st...... existing.txt"), Some("existing.txt"));
    }

    #[test]
    fn extracts_regular_file_paths_openrsync_format() {
        // macOS ships openrsync, which uses a shorter, variable-length code
        // for the same event (observed live: ">f+++++++" for a new file).
        assert_eq!(
            parse_itemized_file(">f+++++++ classifier_gt_eval_threaded.jsonl"),
            Some("classifier_gt_eval_threaded.jsonl")
        );
    }

    #[test]
    fn ignores_directories_and_non_itemized_lines() {
        assert_eq!(parse_itemized_file("cd+++++++++ dir/"), None);
        assert_eq!(parse_itemized_file("sending incremental file list"), None);
        assert_eq!(parse_itemized_file("       1,234 100%   12.34kB/s    0:00:00"), None);
    }
}
