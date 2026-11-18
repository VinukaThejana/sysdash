//! A system dashboard: an Axum server rendering an htmx UI with live CPU, memory, disk,
//! network and process stats, systemd service management and a disk-usage browser.
//!
//! Usage: `sysdash` runs the web server; `sysdash tui` shows the main stats in the terminal, in
//! the terminal theme's colours (`--truecolor` for the web dashboard's exact pastels).
//!
//! Configuration (environment):
//! - `BIND` — defaults to `127.0.0.1:8091`, this machine only. Set it to a private address
//!   (a WireGuard or Tailscale one, say) to open it from other devices. There is no login;
//!   anyone who can open the page can stop services.

mod agents;
mod metrics;
mod routes;
mod storage;
mod systemd;
mod tui;
mod views;

use std::{env, io::IsTerminal};

use routes::{AppState, OwnUid};
use systemd::Systemd;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    match env::args().nth(1).as_deref() {
        None => {}
        // Before logging starts: log lines on stdout would tear through the full-screen view.
        Some("tui") => {
            let truecolor = env::args().skip(2).any(|a| a == "--truecolor");
            let console = env::var("TERM").is_ok_and(|t| t == "linux");
            let palette = match (truecolor, console) {
                (true, _) => &tui::Palette::TRUECOLOR,
                (false, true) => &tui::Palette::CONSOLE,
                (false, false) => &tui::Palette::ANSI,
            };
            return tui::run(palette).await;
        }
        Some("-h" | "--help" | "help") => {
            println!(
                "usage: sysdash [tui [--truecolor]]\n  (none)  run the web dashboard (BIND, default 127.0.0.1:8091)\n  tui     show the main stats in this terminal in its own theme colours; q quits\n          --truecolor uses the web dashboard's exact pastels instead"
            );
            return Ok(());
        }
        Some(other) => {
            eprintln!("sysdash: unknown command {other:?}; try `sysdash --help`");
            std::process::exit(2);
        }
    }

    // Plain output under systemd, so the journal doesn't fill with colour escape codes.
    tracing_subscriber::fmt().with_ansi(std::io::stdout().is_terminal()).init();

    let bind = env::var("BIND").unwrap_or_else(|_| "127.0.0.1:8091".to_owned());
    let home = env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_else(|| "/nonexistent".into());
    let state = AppState {
        snapshot: metrics::spawn(),
        systemd: Systemd::connect().await,
        storage: storage::Storage::new(home),
        agents: agents::Agents::new(),
        own: OwnUid(metrics::own_uid()),
    };

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("sysdash listening on http://{bind}");
    axum::serve(listener, routes::router(state)).await
}
