# ssher

A minimal, modern SSH companion app for macOS. Lives quietly in the menu bar
(no Dock icon) and gives you quick access to remote file browsing, folder
sync, and port forwarding for hosts already in your `~/.ssh/config`.

## Features

- **File system view** — browse remote directories over SFTP
- **File download** — pull individual files to your machine
- **Folder sync** — two-way rsync-over-ssh between a local folder and a remote path
- **Port forwarding** — start/stop local (`-L`) and remote (`-R`) forwards
- **SSH config aliases** — reads `~/.ssh/config` `Host` entries automatically, uses your ssh-agent or identity file for auth
- **Background app** — runs from the menu bar tray; closing the window hides it instead of quitting; open the full window anytime from the tray menu

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
