<p align="center">
  <img src="assets/logo.svg" width="96" height="96" alt="ssher logo" />
</p>

<h1 align="center">ssher</h1>

<p align="center">
  A minimal, modern SSH companion app for macOS — file browsing, folder sync,
  and port forwarding for the hosts already in your <code>~/.ssh/config</code>,
  living quietly in your menu bar.
</p>

## Features

- **File system view** — browse remote directories over SFTP
- **File download** — pull individual files to your machine
- **Folder sync** — two-way rsync-over-ssh between a local folder and a remote path
- **Port forwarding** — start/stop local (`-L`) and remote (`-R`) forwards
- **SSH config aliases** — reads `~/.ssh/config` `Host` entries automatically, uses your ssh-agent or identity file for auth
- **Add New SSH Host** — add a new alias (VS Code Remote-SSH style) straight from the sidebar; it's appended to `~/.ssh/config`
- **Permission guidance** — if the SSH agent has no usable key for a host, ssher tells you exactly what to run and lets you retry
- **Background app** — runs from the menu bar tray, no Dock icon; closing the window hides it instead of quitting; open the full window or quit from the in-app menu or tray

## Stack

- [Tauri 2](https://tauri.app) (Rust backend)
- React + TypeScript (Vite) frontend
- `ssh2` crate for SFTP, system `rsync`/`ssh` binaries for sync and forwarding

## Development

```bash
npm install
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

Produces a `.app` bundle and `.dmg` under `src-tauri/target/release/bundle/`.

## Notes

- Authentication tries your running `ssh-agent` first, then falls back to the
  `IdentityFile` configured for the host alias (defaults to `~/.ssh/id_ed25519`).
- Port forwarding and sync shell out to your system's `ssh`/`rsync` binaries,
  so any `ProxyCommand`/`ProxyJump`/agent-forwarding setup in your SSH config
  is respected automatically.
- If ssher reports a permission/auth error, it will show the exact `ssh-add`
  command to run to grant it access via your agent.

## License

MIT
