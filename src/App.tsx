import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { exit } from "@tauri-apps/plugin-process";
import "./App.css";

type SshHost = {
  alias: string;
  host_name?: string;
  user?: string;
  port?: string;
  identity_file?: string;
};

type ForwardStatus = "connecting" | "active" | "retrying" | "stopped";

type ForwardInfo = {
  id: string;
  alias: string;
  direction: string; // "local" or "remote"
  local_port: number;
  remote_host: string;
  remote_port: number;
  status: ForwardStatus;
  last_error?: string | null;
  retry_count: number;
};

function statusLabel(status: ForwardStatus): string {
  switch (status) {
    case "active":
      return "Active";
    case "connecting":
      return "Connecting…";
    case "retrying":
      return "Retrying…";
    case "stopped":
      return "Stopped";
  }
}

function App() {
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [selectedAlias, setSelectedAlias] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [navMenuOpen, setNavMenuOpen] = useState(false);

  const [showAddHost, setShowAddHost] = useState(false);
  const [newAlias, setNewAlias] = useState("");
  const [newHostName, setNewHostName] = useState("");
  const [newUser, setNewUser] = useState("");
  const [newPort, setNewPort] = useState("");
  const [newIdentityFile, setNewIdentityFile] = useState("");
  const [addHostError, setAddHostError] = useState<string | null>(null);
  const [addingHost, setAddingHost] = useState(false);

  const [forwards, setForwards] = useState<ForwardInfo[]>([]);

  const [showAddForward, setShowAddForward] = useState(false);
  const [fwAlias, setFwAlias] = useState<string | null>(null);
  const [fwLocalPort, setFwLocalPort] = useState("8080");
  const [fwRemoteHost, setFwRemoteHost] = useState("127.0.0.1");
  const [fwRemotePort, setFwRemotePort] = useState("8080");
  const [fwDirection, setFwDirection] = useState<"local" | "remote">("local");
  const [addForwardError, setAddForwardError] = useState<string | null>(null);
  const [addingForward, setAddingForward] = useState(false);

  function refreshHosts() {
    return invoke<SshHost[]>("list_ssh_hosts")
      .then((h) => {
        setHosts(h);
        setSelectedAlias((prev) => prev ?? (h.length ? h[0].alias : null));
        return h;
      })
      .catch((e) => setError(String(e)));
  }

  useEffect(() => {
    refreshHosts();
  }, []);

  useEffect(() => {
    refreshForwards();
    const unlisten = listen<ForwardInfo>("forward-status", (e) => {
      setForwards((prev) => {
        const idx = prev.findIndex((f) => f.id === e.payload.id);
        if (idx === -1) return [...prev, e.payload];
        const next = [...prev];
        next[idx] = e.payload;
        return next;
      });
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  function refreshForwards() {
    invoke<ForwardInfo[]>("list_forwards").then(setForwards).catch(() => {});
  }

  const sortedForwards = useMemo(() => {
    return [...forwards].sort((a, b) => a.alias.localeCompare(b.alias) || a.local_port - b.local_port);
  }, [forwards]);

  function openAddForward(alias?: string) {
    setFwAlias(alias ?? selectedAlias ?? (hosts[0]?.alias || null));
    setAddForwardError(null);
    setShowAddForward(true);
  }

  async function submitAddForward() {
    if (!fwAlias) {
      setAddForwardError("Choose a host");
      return;
    }
    setAddForwardError(null);
    setAddingForward(true);
    try {
      await invoke("start_forward", {
        alias: fwAlias,
        direction: fwDirection,
        localPort: Number(fwLocalPort),
        remoteHost: fwRemoteHost,
        remotePort: Number(fwRemotePort),
      });
      refreshForwards();
      setShowAddForward(false);
    } catch (e) {
      setAddForwardError(String(e));
    } finally {
      setAddingForward(false);
    }
  }

  async function removeForward(id: string) {
    await invoke("stop_forward", { id });
    setForwards((prev) => prev.filter((f) => f.id !== id));
  }

  async function submitAddHost() {
    setAddHostError(null);
    if (!newAlias.trim() || !newHostName.trim()) {
      setAddHostError("Alias and Host are required");
      return;
    }
    setAddingHost(true);
    try {
      await invoke("add_ssh_host", {
        host: {
          alias: newAlias.trim(),
          host_name: newHostName.trim(),
          user: newUser.trim() || undefined,
          port: newPort.trim() || undefined,
          identity_file: newIdentityFile.trim() || undefined,
        },
      });
      setShowAddHost(false);
      setNewAlias("");
      setNewHostName("");
      setNewUser("");
      setNewPort("");
      setNewIdentityFile("");
      const h = await refreshHosts();
      if (h) setSelectedAlias(newAlias.trim());
    } catch (e) {
      setAddHostError(String(e));
    } finally {
      setAddingHost(false);
    }
  }

  async function handleOpen() {
    const win = getCurrentWindow();
    await win.show();
    await win.setFocus();
    setNavMenuOpen(false);
  }

  async function handleQuit() {
    setNavMenuOpen(false);
    await exit(0);
  }

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="sidebar-title">
          <span>ssher</span>
          <div className="nav-menu-wrap">
            <button className="nav-menu-btn" onClick={() => setNavMenuOpen((v) => !v)} aria-label="Menu">
              ⋯
            </button>
            {navMenuOpen && (
              <div className="nav-menu-dropdown" onMouseLeave={() => setNavMenuOpen(false)}>
                <button onClick={handleOpen}>Open</button>
                <button onClick={handleQuit} className="danger-item">
                  Quit
                </button>
              </div>
            )}
          </div>
        </div>

        <div className="host-list">
          {hosts.map((h) => {
            const count = forwards.filter((f) => f.alias === h.alias).length;
            return (
              <button
                key={h.alias}
                className={"host-item" + (h.alias === selectedAlias ? " active" : "")}
                onClick={() => setSelectedAlias(h.alias)}
                onDoubleClick={() => openAddForward(h.alias)}
                title="Click to select, double-click to add a forward"
              >
                <span className="host-alias-row">
                  <span className="host-alias">{h.alias}</span>
                  {count > 0 && <span className="host-count-badge">{count}</span>}
                </span>
                <span className="host-sub">
                  {h.user ? `${h.user}@` : ""}
                  {h.host_name || h.alias}
                </span>
              </button>
            );
          })}
          {hosts.length === 0 && <div className="empty-hint">No hosts found in ~/.ssh/config</div>}
        </div>

        <button className="add-host-btn" onClick={() => setShowAddHost(true)}>
          + Add New SSH Host
        </button>
      </aside>

      <main className="main">
        <div className="main-header">
          <h2>Ports</h2>
          <button className="primary" disabled={hosts.length === 0} onClick={() => openAddForward()}>
            + New Forward
          </button>
        </div>

        {error && <div className="error-banner">{error}</div>}

        <div className="ports-panel">
          {sortedForwards.length === 0 ? (
            <div className="empty-hint">
              No forwards yet. Click <b>+ New Forward</b> to tunnel a port on any host in the sidebar.
            </div>
          ) : (
            <table className="ports-table">
              <thead>
                <tr>
                  <th>Port</th>
                  <th>Host</th>
                  <th>Forwarded Address</th>
                  <th>Status</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {sortedForwards.map((f) => (
                  <tr key={f.id}>
                    <td>
                      {f.direction === "remote" ? "-R " : ""}
                      {f.local_port}
                    </td>
                    <td className="col-host">{f.alias}</td>
                    <td className="col-remote">
                      {f.remote_host}:{f.remote_port}
                    </td>
                    <td title={f.status !== "active" ? f.last_error ?? undefined : undefined}>
                      <span className="status-cell">
                        <span className={"status-dot status-" + f.status} />
                        <span className={"status-text status-" + f.status}>
                          {statusLabel(f.status)}
                          {f.retry_count > 0 && f.status !== "active" ? ` (#${f.retry_count + 1})` : ""}
                        </span>
                      </span>
                    </td>
                    <td className="col-actions">
                      <button className="icon-btn" onClick={() => removeForward(f.id)}>
                        Stop
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </main>

      {showAddForward && (
        <div className="modal-overlay" onClick={() => setShowAddForward(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>New Port Forward</h3>
            <div className="form-row">
              <label>Host</label>
              <select value={fwAlias ?? ""} onChange={(e) => setFwAlias(e.target.value)}>
                {hosts.map((h) => (
                  <option key={h.alias} value={h.alias}>
                    {h.alias}
                  </option>
                ))}
              </select>
            </div>
            <div className="form-row">
              <label>Direction</label>
              <select value={fwDirection} onChange={(e) => setFwDirection(e.target.value as "local" | "remote")}>
                <option value="local">Local forward (-L): access remote service on my machine</option>
                <option value="remote">Remote forward (-R): expose my local service on remote</option>
              </select>
            </div>
            <div className="form-grid">
              <div className="form-row">
                <label>Local port</label>
                <input value={fwLocalPort} onChange={(e) => setFwLocalPort(e.target.value)} />
              </div>
              <div className="form-row">
                <label>Remote host</label>
                <input value={fwRemoteHost} onChange={(e) => setFwRemoteHost(e.target.value)} />
              </div>
              <div className="form-row">
                <label>Remote port</label>
                <input value={fwRemotePort} onChange={(e) => setFwRemotePort(e.target.value)} />
              </div>
            </div>
            {addForwardError && <div className="error-banner">{addForwardError}</div>}
            <div className="modal-actions">
              <button onClick={() => setShowAddForward(false)}>Cancel</button>
              <button className="primary" disabled={addingForward} onClick={submitAddForward}>
                {addingForward ? "Starting…" : "Start forward"}
              </button>
            </div>
          </div>
        </div>
      )}

      {showAddHost && (
        <div className="modal-overlay" onClick={() => setShowAddHost(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Add New SSH Host</h3>
            <p className="modal-hint">Saved as a new Host entry in your ~/.ssh/config.</p>
            <div className="form-row">
              <label>Alias (name)</label>
              <input value={newAlias} onChange={(e) => setNewAlias(e.target.value)} placeholder="my-vm" autoFocus />
            </div>
            <div className="form-row">
              <label>Host / IP address</label>
              <input
                value={newHostName}
                onChange={(e) => setNewHostName(e.target.value)}
                placeholder="203.0.113.10 or vm.example.com"
              />
            </div>
            <div className="form-grid" style={{ gridTemplateColumns: "1fr 1fr" }}>
              <div className="form-row">
                <label>User</label>
                <input value={newUser} onChange={(e) => setNewUser(e.target.value)} placeholder="ubuntu" />
              </div>
              <div className="form-row">
                <label>Port</label>
                <input value={newPort} onChange={(e) => setNewPort(e.target.value)} placeholder="22" />
              </div>
            </div>
            <div className="form-row">
              <label>Identity file (optional)</label>
              <input
                value={newIdentityFile}
                onChange={(e) => setNewIdentityFile(e.target.value)}
                placeholder="~/.ssh/id_ed25519"
              />
            </div>
            {addHostError && <div className="error-banner">{addHostError}</div>}
            <div className="modal-actions">
              <button onClick={() => setShowAddHost(false)}>Cancel</button>
              <button className="primary" disabled={addingHost} onClick={submitAddHost}>
                {addingHost ? "Adding…" : "Add Host"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
