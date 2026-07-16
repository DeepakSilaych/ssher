import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./App.css";

type SshHost = {
  alias: string;
  host_name?: string;
  user?: string;
  port?: string;
  identity_file?: string;
};

type RemoteEntry = {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: number;
};

type ForwardInfo = {
  id: string;
  alias: string;
  direction: string;
  local_port: number;
  remote_host: string;
  remote_port: number;
};

type Tab = "files" | "sync" | "forward";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let val = bytes / 1024;
  let i = 0;
  while (val >= 1024 && i < units.length - 1) {
    val /= 1024;
    i++;
  }
  return `${val.toFixed(1)} ${units[i]}`;
}

function App() {
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [selectedAlias, setSelectedAlias] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("files");
  const [error, setError] = useState<string | null>(null);

  const [remotePath, setRemotePath] = useState("~");
  const [entries, setEntries] = useState<RemoteEntry[]>([]);
  const [loadingDir, setLoadingDir] = useState(false);

  const [localPath, setLocalPath] = useState("");
  const [syncRemotePath, setSyncRemotePath] = useState("~");
  const [direction, setDirection] = useState<"push" | "pull">("pull");
  const [deleteExtra, setDeleteExtra] = useState(false);
  const [syncLog, setSyncLog] = useState<string[]>([]);
  const [syncing, setSyncing] = useState(false);

  const [forwards, setForwards] = useState<ForwardInfo[]>([]);
  const [fwLocalPort, setFwLocalPort] = useState("8080");
  const [fwRemoteHost, setFwRemoteHost] = useState("127.0.0.1");
  const [fwRemotePort, setFwRemotePort] = useState("8080");
  const [fwDirection, setFwDirection] = useState<"local" | "remote">("local");

  useEffect(() => {
    invoke<SshHost[]>("list_ssh_hosts")
      .then((h) => {
        setHosts(h);
        if (h.length) setSelectedAlias((prev) => prev ?? h[0].alias);
      })
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    refreshForwards();
    const unlistenLog = listen<string>("sync-log", (e) => {
      setSyncLog((prev) => [...prev.slice(-500), e.payload]);
    });
    const unlistenDone = listen<boolean>("sync-done", () => {
      setSyncing(false);
    });
    return () => {
      unlistenLog.then((f) => f());
      unlistenDone.then((f) => f());
    };
  }, []);

  function refreshForwards() {
    invoke<ForwardInfo[]>("list_forwards").then(setForwards).catch(() => {});
  }

  async function loadDir(alias: string, path: string) {
    setLoadingDir(true);
    setError(null);
    try {
      const res = await invoke<RemoteEntry[]>("list_dir", { alias, path });
      setEntries(res);
      setRemotePath(path);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingDir(false);
    }
  }

  useEffect(() => {
    if (selectedAlias && tab === "files") {
      loadDir(selectedAlias, "~");
    }
  }, [selectedAlias, tab]);

  async function handleDownload(entry: RemoteEntry) {
    if (!selectedAlias) return;
    const dest = await save({ defaultPath: entry.name });
    if (!dest) return;
    setError(null);
    try {
      await invoke("download_file", {
        alias: selectedAlias,
        remotePath: entry.path,
        localPath: dest,
      });
    } catch (e) {
      setError(String(e));
    }
  }

  async function pickLocalFolder() {
    const dir = await open({ directory: true });
    if (dir) setLocalPath(dir as string);
  }

  async function runSync() {
    if (!selectedAlias || !localPath) return;
    setSyncLog([]);
    setSyncing(true);
    setError(null);
    try {
      await invoke("sync_folder", {
        alias: selectedAlias,
        localPath,
        remotePath: syncRemotePath,
        direction,
        delete: deleteExtra,
      });
    } catch (e) {
      setError(String(e));
      setSyncing(false);
    }
  }

  async function addForward() {
    if (!selectedAlias) return;
    setError(null);
    try {
      await invoke("start_forward", {
        alias: selectedAlias,
        direction: fwDirection,
        localPort: Number(fwLocalPort),
        remoteHost: fwRemoteHost,
        remotePort: Number(fwRemotePort),
      });
      refreshForwards();
    } catch (e) {
      setError(String(e));
    }
  }

  async function removeForward(id: string) {
    await invoke("stop_forward", { id });
    refreshForwards();
  }

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="sidebar-title">ssher</div>
        <div className="host-list">
          {hosts.map((h) => (
            <button
              key={h.alias}
              className={"host-item" + (h.alias === selectedAlias ? " active" : "")}
              onClick={() => setSelectedAlias(h.alias)}
            >
              <span className="host-alias">{h.alias}</span>
              <span className="host-sub">
                {h.user ? `${h.user}@` : ""}
                {h.host_name || h.alias}
              </span>
            </button>
          ))}
          {hosts.length === 0 && <div className="empty-hint">No hosts found in ~/.ssh/config</div>}
        </div>
      </aside>

      <main className="main">
        <div className="tabs">
          {(["files", "sync", "forward"] as Tab[]).map((t) => (
            <button key={t} className={"tab" + (tab === t ? " active" : "")} onClick={() => setTab(t)}>
              {t === "files" ? "Files" : t === "sync" ? "Sync" : "Port Forward"}
            </button>
          ))}
        </div>

        {error && <div className="error-banner">{error}</div>}

        {!selectedAlias && <div className="empty-hint">Select a host to get started</div>}

        {selectedAlias && tab === "files" && (
          <section className="panel">
            <div className="path-bar">
              <input
                value={remotePath}
                onChange={(e) => setRemotePath(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && loadDir(selectedAlias, remotePath)}
              />
              <button onClick={() => loadDir(selectedAlias, remotePath)}>Go</button>
            </div>
            {loadingDir ? (
              <div className="empty-hint">Loading…</div>
            ) : (
              <table className="file-table">
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Size</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((e) => (
                    <tr key={e.path}>
                      <td
                        className={e.is_dir ? "dir-name" : ""}
                        onClick={() => e.is_dir && loadDir(selectedAlias, e.path)}
                      >
                        {e.is_dir ? "📁" : "📄"} {e.name}
                      </td>
                      <td>{e.is_dir ? "" : formatSize(e.size)}</td>
                      <td>{!e.is_dir && <button onClick={() => handleDownload(e)}>Download</button>}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </section>
        )}

        {selectedAlias && tab === "sync" && (
          <section className="panel">
            <div className="form-row">
              <label>Local folder</label>
              <div className="input-with-button">
                <input
                  value={localPath}
                  onChange={(e) => setLocalPath(e.target.value)}
                  placeholder="/Users/you/project"
                />
                <button onClick={pickLocalFolder}>Browse</button>
              </div>
            </div>
            <div className="form-row">
              <label>Remote folder</label>
              <input value={syncRemotePath} onChange={(e) => setSyncRemotePath(e.target.value)} />
            </div>
            <div className="form-row">
              <label>Direction</label>
              <select value={direction} onChange={(e) => setDirection(e.target.value as "push" | "pull")}>
                <option value="pull">Remote → Local (pull)</option>
                <option value="push">Local → Remote (push)</option>
              </select>
            </div>
            <div className="form-row checkbox-row">
              <label>
                <input type="checkbox" checked={deleteExtra} onChange={(e) => setDeleteExtra(e.target.checked)} />
                Delete files at destination not present in source
              </label>
            </div>
            <button className="primary" disabled={syncing || !localPath} onClick={runSync}>
              {syncing ? "Syncing…" : "Sync now"}
            </button>
            <pre className="sync-log">{syncLog.join("\n")}</pre>
          </section>
        )}

        {selectedAlias && tab === "forward" && (
          <section className="panel">
            <div className="form-row">
              <label>Direction</label>
              <select value={fwDirection} onChange={(e) => setFwDirection(e.target.value as "local" | "remote")}>
                <option value="local">Local forward (-L): access remote service on my machine</option>
                <option value="remote">Remote forward (-R): expose my local service on remote</option>
              </select>
            </div>
            <div className="form-row triple">
              <div>
                <label>Local port</label>
                <input value={fwLocalPort} onChange={(e) => setFwLocalPort(e.target.value)} />
              </div>
              <div>
                <label>Remote host</label>
                <input value={fwRemoteHost} onChange={(e) => setFwRemoteHost(e.target.value)} />
              </div>
              <div>
                <label>Remote port</label>
                <input value={fwRemotePort} onChange={(e) => setFwRemotePort(e.target.value)} />
              </div>
            </div>
            <button className="primary" onClick={addForward}>
              Start forward
            </button>

            <div className="forward-list">
              {forwards.map((f) => (
                <div key={f.id} className="forward-item">
                  <span>
                    <b>{f.alias}</b> {f.direction === "remote" ? "-R" : "-L"} {f.local_port} ⇄ {f.remote_host}:
                    {f.remote_port}
                  </span>
                  <button onClick={() => removeForward(f.id)}>Stop</button>
                </div>
              ))}
              {forwards.length === 0 && <div className="empty-hint">No active forwards</div>}
            </div>
          </section>
        )}
      </main>
    </div>
  );
}

export default App;
