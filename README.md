<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="96" height="96" alt="ssher logo" />
</p>

<h1 align="center">ssher</h1>

<p align="center">
  Always-on SSH port forwards to the hosts already in your <code>~/.ssh/config</code>, from the macOS menu bar.
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

A port forward you actually depend on should not be a terminal tab you have to remember. `ssh -N -L` dies on a dropped connection, a network change, or a laptop sleep, and nothing tells you: the command is just gone, and the port stops answering.

ssher keeps forwards up. Each one is supervised, reconnected with backoff when it drops, restored when the app restarts, and reported with a status that is measured rather than assumed.

It also has no host list and no credential store of its own. It reads `~/.ssh/config`, and it shells out to the `ssh` binary already on your machine, so `ProxyCommand`, `ProxyJump`, agent forwarding and `known_hosts` all work the way they already do for you.

It lives in the menu bar with no Dock icon. Closing the window hides it; forwards keep running.

## Demo

<!-- TODO: screenshot of the ports table with a mix of active and retrying forwards -->
<!-- TODO: screenshot of the New Port Forward dialog -->

## Quickstart

### Install the prebuilt app (macOS, Apple Silicon)

1. Download the `.dmg` from [Releases](https://github.com/DeepakSilaych/ssher/releases/latest), open it, drag **ssher** into Applications.

2. The bundle is not code-signed (`src-tauri/tauri.conf.json` sets no signing identity), so Gatekeeper blocks the first launch. Clear the quarantine flag:

```bash
xattr -cr /Applications/ssher.app
```

   Or right-click the app in Finder, choose **Open**, then **Open** again on the warning.

3. Launch it. The ssher icon appears in the menu bar. Click it and choose **Show ssher**.

4. Verify: the sidebar lists the concrete `Host` aliases from `~/.ssh/config`. Click **+ New Forward**, pick a host and a port, and the ports table should show it go **Active**.

If a forward sits on **Retrying**, hover the status for the last error. A common cause is no usable key:

```bash
ssh-add ~/.ssh/id_ed25519
```

Intel builds are not published. Build from source instead (below).

### Build from source

Needs a Rust toolchain, Node.js with npm, and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for macOS. At runtime the app needs `ssh` on `PATH` (macOS ships it).

```bash
git clone https://github.com/DeepakSilaych/ssher.git
cd ssher
npm install
npm run tauri dev
```

`npm run tauri dev` starts Vite on port 1420 (`vite.config.ts`, strict port) and then the Rust app with hot reload for the frontend.

## How it works

```
   ~/.ssh/config                 forwards.json (app config dir)
         |                                |
         | host aliases                   | saved forwards, restored on launch
         v                                v
   +-------------------------------------------------------+
   |  React UI  (src/App.tsx)                              |
   |  sidebar: host list      main: ports table            |
   +-------------------------------------------------------+
        | invoke()                    ^ listen("forward-status")
        v                             |
   +-------------------------------------------------------+
   |  Tauri commands  (src-tauri/src/lib.rs)               |
   +---------------------------+---------------------------+
   | ssh_config.rs             | forward.rs                |
   | parse config, add host    | one supervisor thread per |
   |                           | forward, persisted to disk|
   +---------------------------+---------------------------+
                                           |
                                           v
                         ssh -N -L/-R  (one child per forward)
                         system ssh: ProxyCommand, ProxyJump,
                         known_hosts, agent
```

1. `src-tauri/src/ssh_config.rs` parses `~/.ssh/config`. It collects every concrete (non-wildcard) `Host` alias, then for each alias merges the fields of every matching block, first match wins per directive, the same way OpenSSH does. Only `HostName`, `User`, `Port`, `IdentityFile` and `ProxyCommand` are read.
2. The frontend is a single React component (`src/App.tsx`). It calls Rust through `invoke()` and receives live status through the `forward-status` event, so the table updates without polling.
3. `forward.rs::start_forward` writes the forward to `forwards.json` and hands it to a supervisor thread. `stop_forward` kills the process and removes it from that file. `list_forwards` returns the current state for the initial render.
4. Each supervisor thread owns exactly one forward: spawn `ssh -N -L|-R <local>:<host>:<remote> <alias>`, watch it, and on any failure kill it and respawn after a backoff that doubles from 1 s to a 30 s ceiling. The backoff resets only once the forward is confirmed healthy, so a forward that fails instantly on every attempt backs off instead of hot-looping.
5. Health is probed, not assumed. For a local (`-L`) forward the supervisor TCP-connects to `127.0.0.1:<local_port>` every 3 s; the forward is reported **Active** only when that connect succeeds. A remote (`-R`) forward has no local listener, so only process liveness is checked.
6. On launch `forward::restore_on_startup` reads `forwards.json` and starts a supervisor for every saved forward, so tunnels come back by themselves.
7. `lib.rs` builds the tray icon with **Show ssher** / **Quit**, sets `ActivationPolicy::Accessory` on macOS (no Dock icon), and intercepts the window close event to hide instead of quit.

### The ssh flags, and why

`forward.rs` passes a specific set of options. Each one is there because of a failure it fixes, and two obvious-looking options are deliberately absent:

| Flag | Why |
|---|---|
| `ControlMaster=no`, `ControlPath=none` | With a shared master connection active, `ssh -N -L` hands the forward to the master and exits `0` immediately. The supervisor would read that as a crash and reconnect forever while the tunnel was actually fine. Each forward gets its own connection. |
| `ServerAliveInterval=10`, `ServerAliveCountMax=3` | Makes a dead peer surface as a process exit within ~30 s instead of hanging on a half-open socket. |
| no `ExitOnForwardFailure` | It tears down the whole connection when *any* forward fails, including an unrelated static `RemoteForward` in the host's own `ssh_config` block that another session already holds. The TCP probe checks our forward specifically instead. |
| no `ClearAllForwardings` | It looks like the fix for the line above, but it clears command-line `-L`/`-R` too, leaving a live connection with no forward at all — and with process-liveness status, that reads as healthy. |

## Features

| Feature | Where | Notes |
|---|---|---|
| Host list from `~/.ssh/config` | `ssh_config.rs::parse_ssh_config` | Wildcard patterns like `Host *` contribute defaults but are not listed |
| Add New SSH Host | `ssh_config.rs::add_ssh_host` | Appends a `Host` block; rejects empty or duplicate alias |
| Local (`-L`) and remote (`-R`) forwards | `forward.rs::start_forward` | Any number, across any number of hosts, at once |
| Automatic reconnect | `forward.rs::supervise` | Exponential backoff, 1 s doubling to a 30 s ceiling, indefinitely |
| Survives app restart | `forward.rs::restore_on_startup` | Saved forwards are reconnected on launch |
| Live status per forward | `forward-status` event, `App.tsx` | Connecting / Active / Retrying / Stopped, with last error and retry count |
| Real health check | `forward.rs::supervise` | Active means a TCP connect to the local port succeeded, not just that a process exists |
| Ports table | `App.tsx` | Every forward on every host in one view: port, host, forwarded address, status |
| Menu bar app | `lib.rs` | Tray menu, no Dock icon, close hides the window |

## Configuration

Hosts come from `~/.ssh/config`. Forwards are the one thing ssher stores itself.

| Source | Key | Default | Used for |
|---|---|---|---|
| `~/.ssh/config` | `Host` | | Alias list. Patterns with `*` or `?` are skipped as entries but still supply defaults |
| `~/.ssh/config` | `HostName`, `User`, `Port`, `IdentityFile`, `ProxyCommand` | | Shown in the sidebar; applied by system `ssh` when a forward connects |
| `forwards.json` | | `[]` | Saved forwards, restored on launch. macOS: `~/Library/Application Support/ssher/forwards.json` |
| env | `SSH_AUTH_SOCK` | | ssh-agent socket, used by system `ssh` |
| env | `TAURI_DEV_HOST` | unset | Dev only: bind Vite to a LAN host for on-device testing (`vite.config.ts`) |
| `forward.rs` | `MAX_BACKOFF_SECS` | 30 | Reconnect backoff ceiling |
| `tauri.conf.json` | `app.windows[0]` | 800 x 600 | Main window size |
| `App.tsx` | UI defaults | forward `8080:127.0.0.1:8080`, direction local | Initial dialog values |

Two things are written to disk: `forwards.json`, and the `Host` block that `add_ssh_host` appends to `~/.ssh/config`. No keys, passwords or session data are stored.

## Design decisions and trade-offs

- **One `ssh` child per forward, not a library.** Spawning system `ssh` rather than re-implementing OpenSSH means `ProxyCommand`, `ProxyJump`, agent forwarding and `known_hosts` work for free, and the host's own config is honoured exactly. The cost is a process per forward and parsing behaviour out of exit codes.
- **Status is measured.** A supervised process being alive turned out to be a poor proxy for a working tunnel: ssh will happily hold a connection open with no listener bound. Reporting **Active** only on a successful TCP connect to the local port is the difference between a status light and a status guess. It does mean a `-R` forward, which has no local listener, gets the weaker liveness-only check.
- **Backoff resets on health, not on spawn.** Resetting the delay whenever a process starts successfully lets a forward that dies immediately retry roughly once a second forever. The reset is tied to the forward being confirmed healthy instead.
- **Stop means delete.** Stopping a forward removes it from `forwards.json` rather than parking it in a disabled state, so a stopped forward stays stopped across restarts. There is no way to keep a forward in the list but switch it off.
- **Its own connection per forward, deliberately.** Opting out of `ControlMaster` costs a real handshake per forward and gives up connection sharing, in exchange for a process whose lifetime actually tracks the tunnel.
- **ssh_config merge semantics are replicated, not approximated.** A host whose settings are split across a shared block and a specific block shows up once with merged fields (`ssh_config.rs`, tested in `merges_fields_across_matching_blocks`). Only five directives are read; `Include` is not followed.

## Project layout

```
ssher/
  index.html              Vite entry
  package.json            npm scripts: dev, build, tauri
  vite.config.ts          port 1420, strictPort, ignores src-tauri
  src/
    main.tsx              React root
    App.tsx               whole UI: sidebar, ports table, two modals
    App.css
  src-tauri/
    Cargo.toml            tauri 2 (tray-icon), dirs, uuid, serde
    tauri.conf.json       product name, window, bundle targets, icons
    capabilities/default.json   window show/hide/focus, opener, process exit
    icons/                app icons used by the bundle and the tray
    src/
      main.rs             calls ssher_lib::run()
      lib.rs              Tauri builder, command registry, tray, close-to-hide
      ssh_config.rs       ~/.ssh/config parser + add_ssh_host
      forward.rs          supervisor threads, health probe, persistence
```

## Development

Run in dev mode (Vite hot reload for the frontend, Rust rebuild on change):

```bash
npm install
npm run tauri dev
```

Rust unit tests (config merge, wildcard skipping):

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

- Port forwarding is the whole app. Earlier versions had an SFTP file browser and an rsync folder sync; both were removed.
- Remote (`-R`) forwards get liveness-only status. The TCP probe checks a local listener, which a `-R` forward does not have, so a `-R` forward can read **Active** while the remote listener is gone.
- The probe confirms the tunnel's local end. If nothing is listening on the target port on the remote host, the forward is still **Active** — connections through it will just fail.
- Quitting does not clean up. `app.exit(0)` does not kill the `ssh -N` children, so they can outlive the app and keep holding their local ports. Stop forwards before quitting, or kill the strays.
- No pause. Stopping a forward deletes it; there is no disabled-but-remembered state.
- `ProxyJump` and `Include` are not parsed by the config reader, so such hosts may not appear in the sidebar even though system `ssh` would handle them.
- `ActivationPolicy::Accessory` is macOS only. Other platforms are not targeted even though `bundle.targets` is `"all"`.
- The app is unsigned. Apple Silicon `.dmg` only in Releases.
- `Cargo.toml` still carries the Tauri template `description = "A Tauri App"` and `authors = ["you"]`.

## Contributing

Open an issue or a PR on [GitHub](https://github.com/DeepakSilaych/ssher). Keep Rust changes covered by a unit test where the logic is parseable (see `ssh_config.rs` for the pattern).

## License

MIT. See [LICENSE](LICENSE).
