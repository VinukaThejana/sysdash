# sysdash

A self-hosted system dashboard for one Linux machine, styled like kodi-remote. Open it on
a phone or laptop to see how the machine is doing and to manage its services.

- **Overview**: CPU (overall, per thread, load, temperature) and memory with 2-minute
  charts, swap, disks and network rates. Refreshes every 2 seconds.
- **Needs attention**: services that failed or are crash-looping (still starting after 3+
  automatic restarts) are listed at the top.
- **Processes**: the top 15 by CPU or memory. Your own processes get a stop button
  (SIGTERM, so they can shut down cleanly).
- **Services**: every system and user service, filtered by running/failed/stopped and
  searchable. Tap one for its state, memory, CPU time, restarts, unit file and the last
  200 journal lines, plus Start/Stop/Restart, Kill (SIGKILL to all its processes) and
  "start at boot" toggles. Every destructive action asks first, with an extra warning for
  units that keep the machine reachable (sshd, NetworkManager, wg-quick, …).
- **Delete**: systemd has no delete, so it does the strongest safe step for where the unit
  lives. Your own user units (`~/.config/systemd/user`) are stopped, disabled and their
  file removed. Packaged units and template instances are stopped and masked (Unmask
  undoes it). System units whose file is in `/etc/systemd/system` belong to root: they are
  stopped and disabled, and you get the `sudo rm …` command to remove the file.
- **Agents**: every AI coding agent running on the machine. Agents in
  [herdr](https://herdr.dev) (optional) show their live
  state (waiting for you, working, done, idle), the task title the agent set, and their
  workspace and folder; agent processes outside herdr (claude, codex, opencode, gemini,
  aider and others, found by process name) show as "running". Each agent's CPU and memory
  include everything it spawned. Waiting-for-you agents are listed first. A tab beside
  Processes and Storage on laptops, a bottom tab on phones, and a section in the terminal
  view and `curl` text.
- **Storage**: a disk-usage browser for the whole filesystem (a tab on phones; on laptops it
  shares a tab strip with Processes; tapping a Disk tile opens its mount). The first visit starts a
  background scan (about 30 s cold, a few seconds warm) that indexes every folder's size the
  way `du` does, counting hard links once and skipping virtual filesystems. Folders and files
  are listed largest first with share bars. Anything inside your home folder can be deleted
  permanently, after a confirmation showing its size and file count; everything outside home
  is view-only, and a path that leads outside home through a symlink is refused. Rescan
  refreshes the index; it is dropped after 30 idle minutes to free memory. On btrfs with
  compression, sizes are uncompressed (like `du`), so they add up to more than `df` shows.

It's a small Rust server ([Axum](https://github.com/tokio-rs/axum)) rendering HTML for
[htmx](https://htmx.org). Stats come from `/proc` via
[sysinfo](https://crates.io/crates/sysinfo), services from systemd's D-Bus API, and logs
from `journalctl`. Everything it serves is compiled into one binary.

## Terminal view

`sysdash tui` shows the main stats (host, failing services, CPU with per-thread load,
memory, swap, disks, network, and two minutes of CPU and memory history) as a live
full-screen view in any terminal, from the same binary and with the same numbers as the web
page. q, Esc or Ctrl-C quits. It samples on its own, so the web server doesn't need to run.

The web server answers the same screen as text to anything that doesn't ask for HTML, so
`curl localhost:8091` prints it once with colours. `?width=72` sets the columns (40 to 200,
default 100) and `?plain` drops the colour codes. Browsers still get the web dashboard.

Colours come from the terminal's own 16-colour palette (CPU blue, memory green, disk magenta,
network cyan, failure red, warnings yellow), so both views follow your theme, light or dark.
`sysdash tui --truecolor` and `curl 'localhost:8091/?truecolor'` use the web dashboard's exact
pastels instead.

`deploy/console.sh` (run with sudo) shows the view on tty1 in place of the login prompt, as a
service with no login session or shell, and switches the console font to Terminus.
`--undo` puts the login prompt back.

## Requirements

- Linux with systemd (services and logs come from systemd and `journalctl`)
- A recent stable [Rust](https://rustup.rs), to build it
- Optional: [herdr](https://herdr.dev), for live agent states

## Install

```sh
git clone https://github.com/VinukaThejana/sysdash.git
cd sysdash
cargo build --release
install -Dm755 target/release/sysdash ~/.local/bin/sysdash
install -Dm644 deploy/sysdash.service ~/.config/systemd/user/sysdash.service
systemctl --user enable --now sysdash
```

It now runs at <http://127.0.0.1:8091>, on this machine only. To open it from your phone or
laptop, make it listen on a private address those devices reach, such as the machine's
WireGuard or Tailscale address (see [Configuration](#configuration) and
[Security](#security)):

```sh
mkdir -p ~/.config/sysdash
echo 'BIND=10.10.0.1:8091' > ~/.config/sysdash/env   # your VPN address
systemctl --user restart sysdash
```

To keep it running after you log out (and start it at boot), enable lingering once:
`loginctl enable-linger $USER`.

On an iPhone, Share → Add to Home Screen makes it behave like an app.

### Behind nginx

`deploy/nginx.example.conf` serves it at the site root through nginx and lets only a VPN
subnet in.

### Managing system services

Your own (user) services work straight away. Starting or stopping system services as a
normal user needs polkit's permission, which normally means a password prompt the
dashboard can't show. Install the rule to allow it:

```sh
sed "s/@USER@/$USER/" deploy/50-sysdash.rules | sudo install -Dm644 /dev/stdin /etc/polkit-1/rules.d/50-sysdash.rules
```

Run it as the user the dashboard runs as. Without the rule, those buttons answer "Not
allowed".

Logs of system services need the `wheel`, `adm` or `systemd-journal` group.

## Configuration

Environment variables, set in `~/.config/sysdash/env` when running as a service
(`systemctl --user restart sysdash` after changing it):

| Variable | Default | Meaning |
|---|---|---|
| `BIND` | `127.0.0.1:8091` | Address and port to listen on |

## Security

There is no login: anyone who can open the page can stop services. By default it listens
on `127.0.0.1` only. Bind it to an address only you can reach, such as a WireGuard or
Tailscale one, and don't bind it to `0.0.0.0` on a network you don't control.

The polkit rule applies to every process running as your user, not just the dashboard:
with it installed, anything you run can manage system services without `sudo`.

## Development

```sh
BIND=127.0.0.1:18091 cargo run
cargo test
```

Layout:

- `src/metrics.rs`: background sampler publishing a stats snapshot every 2 s
- `src/systemd.rs`: service list, details, actions and logs
- `src/storage.rs`: the disk-usage scan, its index, browsing and deletes
- `src/agents.rs`: herdr's agents joined with agent process stats
- `src/views.rs`: view models and formatting for the templates
- `src/routes.rs`: HTTP routes
- `templates/`, `assets/`: Askama templates, CSS and JS (htmx + idiomorph vendored)
