<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="96" height="96" alt="ssher logo" />
</p>

<h1 align="center">ssher</h1>

<p align="center">
  Browse, sync and port-forward to the hosts already in your <code>~/.ssh/config</code>, from the macOS menu bar.
</p>

<p align="center">
  <a href="https://github.com/DeepakSilaych/ssher/releases/latest">
    <img alt="Download for macOS (Apple Silicon)" src="https://img.shields.io/badge/Download-macOS%20(Apple%20Silicon)-3b82f6?style=for-the-badge&logo=apple">
  </a>
  <a href="LICENSE">
    <img alt="MIT license" src="https://img.shields.io/badge/license-MIT-green?style=for-the-badge">
  </a>
</p>

## Why

You already keep your hosts, users, ports and keys in `~/.ssh/config`. Most GUI SSH clients want you to type all of that again into their own bookmark list and key store.

ssher has no host list and no credential store of its own. It reads `~/.ssh/config`, authenticates through your running `ssh-agent` (or the host's `IdentityFile`), and for anything that needs OpenSSH semantics (`ProxyCommand`, `ProxyJump`, `known_hosts`) it shells out to the `ssh` and `rsync` binaries already on your machine.

It lives in the menu bar with no Dock icon. Closing the window hides it; forwards keep running.

## Demo

<!-- TODO: screenshot of the Files tab browsing a remote host -->
<!-- TODO: screenshot of the Sync tab with rsync log output -->
<!-- TODO: screenshot of the Port Forward tab with an active -L forward -->

## Quickstart

### Install the prebuilt app (macOS, Apple Silicon)

1. Download the `.dmg` from [Releases](https://github.com/DeepakSilaych/ssher/releases/latest), open it, drag **ssher** into Applications.

2. The bundle is not code-signed (`src-tauri/tauri.conf.json` sets no signing identity), so Gatekeeper blocks the first launch. Clear the quarantine flag:

```bash
xattr -cr /Applications/ssher.app
```

   Or right-click the app in Finder, choose **Open**, then **Open** again on the warning.

3. Launch it. The ssher icon appears in the menu bar. Click it and choose **Show ssher**.

4. Verify: the sidebar lists the concrete `Host` aliases from `~/.ssh/config`. Pick one and the Files tab lists its home directory over SFTP.

If the sidebar shows **No SSH agent detected**, load a key first:

```bash
ssh-add ~/.ssh/id_ed25519
```

Intel builds are not published. Build from source instead (below).

### Build from source

Needs a Rust toolchain, Node.js with npm, and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for macOS. At runtime the app needs `ssh` and `rsync` on `PATH` (macOS ships both).

```bash
git clone https://github.com/DeepakSilaych/ssher.git
cd ssher
npm install
npm run tauri dev
```

`npm run tauri dev` starts Vite on port 1420 (`vite.config.ts`, strict port) and then the Rust app with hot reload for the frontend.

## How it works

```
                 ~/.ssh/config
                       |
                       | read on startup + after "Add New SSH Host"
                       v
   +-------------------------------------------------------+
   |  React UI  (src/App.tsx)                              |
   |  sidebar: host list        tabs: Files | Sync | Fwd   |
   +-------------------------------------------------------+
        | invoke()                        ^ listen("sync-log", "sync-done")
        v                                 |
   +-------------------------------------------------------+
   |  Tauri commands  (src-tauri/src/lib.rs)               |
   +-----------+------------------+------------------------+
   | sftp.rs   | sync.rs          | forward.rs             |
   | ssh2 crate| spawn rsync      | spawn ssh -N -L / -R   |
   | in-process| -avz --itemize   | one child per forward  |
   | libssh2   | over system ssh  | tracked in a HashMap   |
   +-----------+------------------+------------------------+
        |                |                     |
        v                v                     v
   ssh-agent, then   system ssh: honours ProxyCommand,
   IdentityFile      ProxyJump, known_hosts, agent
```

1. `src-tauri/src/ssh_config.rs` parses `~/.ssh/config`. It collects every concrete (non-wildcard) `Host` alias, then for each alias merges the fields of every matching block, first match wins per directive, the same way OpenSSH does. Only `HostName`, `User`, `Port`, `IdentityFile` and `ProxyCommand` are read.
2. The frontend is a single React component (`src/App.tsx`). It calls Rust through `invoke()` and receives sync output through two events, `sync-log` and `sync-done`.
3. **Files** tab: `sftp.rs::connect` opens a TCP socket (8 s timeout), does the SSH handshake with the `ssh2` crate, tries every identity in the agent, then falls back to the host's `IdentityFile`. `list_dir` and `download_file` each open a fresh session. `~` is expanded through SFTP `realpath(".")` because SFTP has no shell.
4. **Sync** tab: `sync.rs::sync_folder` first creates the destination folder (`create_dir_all` locally, `ssh <alias> mkdir -p` remotely), then runs `rsync -avz --itemize-changes --progress [--delete] src dst` with `<alias>:<path>` as the remote side. stdout and stderr are streamed to the UI line by line. Itemized lines for regular files are collected and, if the checkbox is on, their absolute local paths are put on the clipboard with `pbcopy`.
5. **Port Forward** tab: `forward.rs::start_forward` spawns `ssh -N -L|-R <local>:<host>:<remote> <alias>`, stores the child under a UUID in `ForwardState`, and `stop_forward` kills it.
6. `lib.rs` builds the tray icon with **Show ssher** / **Quit**, sets `ActivationPolicy::Accessory` on macOS (no Dock icon), and intercepts the window close event to hide instead of quit.

## Features

| Feature | Where | Notes |
|---|---|---|
| Host list from `~/.ssh/config` | `ssh_config.rs::parse_ssh_config` | Wildcard patterns like `Host *` contribute defaults but are not listed |
| Add New SSH Host | `ssh_config.rs::add_ssh_host` | Appends a `Host` block; rejects empty or duplicate alias |
| Remote directory browsing (SFTP) | `sftp.rs::list_dir` | Directories first, then by name; path bar accepts `~` and `~/x` |
| File download | `sftp.rs::download_file` | Save dialog via `tauri-plugin-dialog` |
| Folder sync, push or pull | `sync.rs::sync_folder` | rsync over system ssh; optional `--delete` |
| Changed-file paths to clipboard | `sync.rs::parse_itemized_file` | Parses both GNU rsync and macOS openrsync itemize codes |
| Local (`-L`) and remote (`-R`) forwards | `forward.rs` | Start, stop, list; one `ssh -N` process each |
| Agent check and fix hint | `sftp.rs::check_ssh_agent`, `App.tsx` | Warns when no agent; on `PERMISSION_DENIED` shows the exact `ssh-add <IdentityFile>` to run and a Retry button |
| Menu bar app | `lib.rs` | Tray menu, no Dock icon, close hides the window |

## Configuration

ssher has no config file of its own. Everything comes from `~/.ssh/config`, the environment, or constants in the source.

| Source | Key | Default | Used for |
|---|---|---|---|
| `~/.ssh/config` | `Host` | | Alias list. Patterns with `*` or `?` are skipped as entries but still supply defaults |
| `~/.ssh/config` | `HostName` | the alias | SFTP connect target |
| `~/.ssh/config` | `User` | `$USER` | SFTP login user |
| `~/.ssh/config` | `Port` | `22` | SFTP connect port |
| `~/.ssh/config` | `IdentityFile` | `~/.ssh/id_ed25519` | Fallback key when the agent fails; shown in the `ssh-add` hint |
| `~/.ssh/config` | `ProxyCommand` | | Files tab refuses the host with an explanatory error; Sync and Forward work because system `ssh` handles it |
| env | `SSH_AUTH_SOCK` | | ssh-agent socket, used by libssh2 (Files) and by system `ssh` (Sync, Forward) |
| env | `USER` | | Fallback SSH user when `User` is not set |
| env | `TAURI_DEV_HOST` | unset | Dev only: bind Vite to a LAN host for on-device testing (`vite.config.ts`) |
| `sftp.rs` | `CONNECT_TIMEOUT` | 8 s | TCP connect timeout |
| `sftp.rs` | `SESSION_TIMEOUT_MS` | 10000 | libssh2 session timeout |
| `tauri.conf.json` | `app.windows[0]` | 800 x 600 | Main window size |
| `App.tsx` | UI defaults | remote path `~`, direction pull, forward `8080:127.0.0.1:8080`, clipboard copy on | Initial form values |

Only one thing is ever written to disk: `add_ssh_host` appends to `~/.ssh/config`. No keys, passwords or session data are stored.

## Design decisions and trade-offs

- **Two transports on purpose.** SFTP runs in-process through the `ssh2` crate (libssh2) so directory listing needs no shell and no subprocess. Sync and forwarding spawn the system `ssh` and `rsync` instead of re-implementing OpenSSH, so `ProxyCommand`, `ProxyJump`, agent forwarding and `known_hosts` all work for free. The cost: the Files tab refuses `ProxyCommand` hosts (`sftp.rs::connect`) and does not know about `ProxyJump` at all, because the parser does not read it.
- **No stored credentials.** Auth is agent first, then `IdentityFile` with no passphrase (`userauth_pubkey_file(..., None)`). A passphrase-protected key only works if it is loaded in the agent, which is why the app surfaces the `ssh-add` command instead of asking for a passphrase.
- **No host-key verification on the libssh2 path.** `sftp.rs::connect` handshakes without consulting `~/.ssh/known_hosts`. The Sync and Forward paths get host-key checking from system `ssh`. Treat the Files tab as trusting the network.
- **One session per command.** `list_dir` and `download_file` each do a full connect and auth. Simple and stateless, but every directory click pays a handshake.
- **ssh_config merge semantics are replicated, not approximated.** A host whose settings are split across a shared block and a specific block shows up once with merged fields (`ssh_config.rs`, tested in `merges_fields_across_matching_blocks`). Only five directives are read; `Include` is not followed.
- **rsync itemize parsing splits on the first space.** GNU rsync pads the change code to 11 chars; macOS `openrsync` emits a shorter one. Both are handled (`sync.rs::parse_itemized_file`, with tests for each).
- **Static OpenSSL.** `ssh2` is built with `vendored-openssl`, so the `.dmg` does not depend on a Homebrew OpenSSL. First build is slower.

## Project layout

```
ssher/
  index.html              Vite entry
  package.json            npm scripts: dev, build, tauri
  vite.config.ts          port 1420, strictPort, ignores src-tauri
  src/
    main.tsx              React root
    App.tsx               whole UI: sidebar, three tabs, two modals
    App.css
  src-tauri/
    Cargo.toml            tauri 2 (tray-icon), ssh2 0.9, dirs, uuid, shellexpand
    tauri.conf.json       product name, window, bundle targets, icons
    capabilities/default.json   window show/hide/focus, dialog, opener, process exit
    icons/                app icons used by the bundle and the tray
    src/
      main.rs             calls ssher_lib::run()
      lib.rs              Tauri builder, command registry, tray, close-to-hide
      ssh_config.rs       ~/.ssh/config parser + add_ssh_host
      sftp.rs             connect, auth, list_dir, download_file, check_ssh_agent
      sync.rs             rsync runner, itemize parser, pbcopy
      forward.rs          ssh -N forward processes
```

## Development

Run in dev mode (Vite hot reload for the frontend, Rust rebuild on change):

```bash
npm install
npm run tauri dev
```

Rust unit tests (config merge, wildcard skipping, rsync itemize parsing):

```bash
cd src-tauri && cargo test
```

Type-check the frontend (strict TS, unused locals and params are errors):

```bash
npx tsc
```

Production build. Runs `npm run build` (`tsc && vite build`) into `dist/`, then compiles the Rust binary and bundles it:

```bash
npm run tauri build
```

Output lands under `src-tauri/target/release/bundle/` (`.app` and `.dmg` on macOS, since `bundle.targets` is `"all"`).

There is no CI workflow and no release automation in the repo. Releases are built locally with the command above and uploaded by hand.

## Limitations

All of these are visible in the code today.

- Files tab is download only. No upload, rename, delete or mkdir over SFTP.
- `download_file` reads the whole remote file into memory before writing it, so very large files are limited by RAM.
- No `known_hosts` check in the Files tab (see trade-offs).
- `ProxyCommand` hosts are rejected in the Files tab; `ProxyJump` and `Include` directives are not parsed.
- Forwards live only in memory. Quitting the app calls `app.exit(0)` without killing the `ssh -N` children, so stop forwards from the Port Forward tab before quitting.
- Clipboard copy uses `pbcopy`, so it is macOS only. `ActivationPolicy::Accessory` is also macOS only. Other platforms are not targeted even though `bundle.targets` is `"all"`.
- The app is unsigned. Apple Silicon `.dmg` only in Releases.
- `Cargo.toml` still carries the Tauri template `description = "A Tauri App"` and `authors = ["you"]`.

## Contributing

Open an issue or a PR on [GitHub](https://github.com/DeepakSilaych/ssher). Keep Rust changes covered by a unit test where the logic is parseable (see `ssh_config.rs` and `sync.rs` for the pattern).

## License

MIT. See [LICENSE](LICENSE).
