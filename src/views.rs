//! Askama templates and the view models they render. Numbers are formatted here so the
//! templates only lay things out.

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use askama::Template;
use serde::Deserialize;

use crate::{
    metrics::{Battery, Charge, Process, Snapshot, Usage},
    storage::{Listing, Storage},
    agents::AgentList,
    systemd::{DeleteMode, Details, Failing, Scope, Service},
};

/// Processes shown in the list.
const PROCESS_ROWS: usize = 15;

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexPage {
    pub host: String,
    /// The live sections, pre-rendered from their own templates.
    pub band: String,
    pub overview: String,
    pub processes: String,
}

/// The polled sky-blue band: who the machine is and the one status sentence.
#[derive(Template)]
#[template(path = "band.html")]
pub struct Band {
    pub host: String,
    pub os: String,
    pub kernel: String,
    pub uptime: String,
    pub failing: Vec<FailingUnit>,
}

impl Band {
    pub fn new(s: &Snapshot, failing: Vec<Failing>) -> Self {
        Self {
            host: s.host.name.clone(),
            os: s.host.os.clone(),
            kernel: s.host.kernel.clone(),
            uptime: duration(s.uptime_secs),
            failing: failing
                .into_iter()
                .map(|f| FailingUnit {
                    url: format!("services/{}/{}", f.scope, f.name),
                    name: f.name.as_str().trim_end_matches(".service").to_owned(),
                    restarting: f.restarting,
                })
                .collect(),
        }
    }
}

/// The polled vitals grid and history.
#[derive(Template)]
#[template(path = "overview.html")]
pub struct Overview {
    pub cpu_brand: String,
    pub cpu: Meter,
    pub cpu_chart: Chart,
    pub cores: Vec<Meter>,
    pub core_summary: String,
    /// One-minute load, then the five- and fifteen-minute figures.
    pub load_1: String,
    pub load_5_15: String,
    pub temperature: Option<String>,
    pub memory: Meter,
    pub memory_text: String,
    pub memory_chart: Chart,
    pub swap: Option<(Meter, String)>,
    pub battery: Option<BatteryTile>,
    pub disks: Vec<DiskRow>,
    pub networks: Vec<NetworkRow>,
    /// All interfaces together, per second.
    pub net_rx: String,
    pub net_tx: String,
}

/// A 0–100 reading with its label and how worrying it is.
pub struct Meter {
    pub percent: u8,
    pub label: String,
    /// `ok`, `warn` or `crit`; styles the bar.
    pub level: &'static str,
}

impl Meter {
    fn new(value: f32) -> Self {
        let percent = value.clamp(0.0, 100.0).round() as u8;
        let level = match percent {
            0..70 => "ok",
            70..90 => "warn",
            _ => "crit",
        };
        Self { percent, label: format!("{percent}%"), level }
    }
}

/// A history of 0–100 values as flat columns, newest on the right, drawn as one SVG path in
/// a 120 × 40 view box (one 2-unit slot per sample) that the page stretches to size.
pub struct Chart {
    pub columns: String,
}

impl Chart {
    const H: f32 = 40.0;
    const SLOTS: usize = 60;
    const SLOT: f32 = 2.0;
    const COLUMN: f32 = 1.5;

    fn new(values: &[f32]) -> Self {
        let offset = Self::SLOTS.saturating_sub(values.len());
        let columns = values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let x = (offset + i) as f32 * Self::SLOT + (Self::SLOT - Self::COLUMN) / 2.0;
                let h = (v.clamp(0.0, 100.0) / 100.0 * Self::H).max(0.4);
                format!("M{x:.2},{H}V{top:.2}h{w}V{H}Z", H = Self::H, top = Self::H - h, w = Self::COLUMN)
            })
            .collect();
        Self { columns }
    }
}

/// The battery's charge, what it's doing, and the rate and wear for its tooltip.
pub struct BatteryTile {
    /// `warn` below 20% and `crit` below 10%, but only while running on it.
    pub meter: Meter,
    /// `On battery · 3h 35m left`, `Charging · 40m to full`.
    pub note: String,
    /// `5.2 W · health 81% · 383 cycles`.
    pub detail: String,
}

impl BatteryTile {
    fn new(b: &Battery) -> Self {
        let mut meter = Meter::new(b.percent);
        meter.level = match (b.state, meter.percent) {
            (Charge::Discharging, 0..10) => "crit",
            (Charge::Discharging, 10..20) => "warn",
            _ => "ok",
        };
        let time = |suffix: &str| b.remaining_secs.map(|s| format!(" · {} {suffix}", duration(s))).unwrap_or_default();
        let note = match b.state {
            Charge::Discharging => format!("On battery{}", time("left")),
            Charge::Charging => format!("Charging{}", time("to full")),
            Charge::Full => "Full · plugged in".to_owned(),
            Charge::NotCharging => "Plugged in · not charging".to_owned(),
        };
        let detail = [
            b.watts.map(|w| format!("{w:.1} W")),
            b.health.map(|h| format!("health {h:.0}%")),
            b.cycles.map(|c| format!("{c} cycles")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        Self { meter, note, detail }
    }
}

pub struct DiskRow {
    pub mount: String,
    pub fs: String,
    pub text: String,
    pub meter: Meter,
}

pub struct NetworkRow {
    pub name: String,
}

pub struct FailingUnit {
    pub name: String,
    pub url: String,
    pub restarting: bool,
}

impl Overview {
    pub fn new(s: &Snapshot) -> Self {
        Self {
            cpu_brand: s.host.cpu_brand.clone(),
            cpu: Meter::new(s.cpu),
            cpu_chart: Chart::new(&s.cpu_history),
            cores: s.cores.iter().map(|c| Meter::new(*c)).collect(),
            core_summary: format!(
                "{} {}",
                s.cores.len(),
                if s.host.physical_cores == s.cores.len() { "cores" } else { "threads" }
            ),
            load_1: format!("{:.2}", s.load[0]),
            load_5_15: format!("5m {:.2} · 15m {:.2}", s.load[1], s.load[2]),
            temperature: s.temperatures.first().map(|t| format!("{:.0}°C", t.celsius)),
            memory: Meter::new(s.memory.percent()),
            memory_text: usage(s.memory),
            memory_chart: Chart::new(&s.memory_history),
            swap: (s.swap.total > 0).then(|| (Meter::new(s.swap.percent()), usage(s.swap))),
            battery: s.battery.as_ref().map(BatteryTile::new),
            disks: s
                .disks
                .iter()
                .map(|d| DiskRow {
                    mount: d.mount.clone(),
                    fs: d.fs.clone(),
                    text: usage(d.space),
                    meter: Meter::new(d.space.percent()),
                })
                .collect(),
            networks: s
                .networks
                .iter()
                .map(|n| NetworkRow { name: n.name.clone() })
                .collect(),
            net_rx: rate(s.networks.iter().map(|n| n.rx_rate).sum()),
            net_tx: rate(s.networks.iter().map(|n| n.tx_rate).sum()),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessSort {
    #[default]
    Cpu,
    Memory,
}

/// The polled top-processes list.
#[derive(Template)]
#[template(path = "processes.html")]
pub struct ProcessList {
    pub by_memory: bool,
    pub rows: Vec<ProcessRow>,
}

pub struct ProcessRow {
    pub pid: u32,
    pub name: String,
    pub command: String,
    pub user: String,
    pub cpu: Meter,
    pub cpu_text: String,
    pub memory: String,
    pub memory_meter: Meter,
    /// Bar lengths, 0–100, relative to the busiest process in the list.
    pub cpu_bar: u8,
    pub memory_bar: u8,
    pub own: bool,
}

impl ProcessList {
    pub fn new(s: &Snapshot, sort: ProcessSort) -> Self {
        let mut processes: Vec<&Process> = s.processes.iter().collect();
        match sort {
            ProcessSort::Cpu => processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu)),
            ProcessSort::Memory => processes.sort_by_key(|p| std::cmp::Reverse(p.memory)),
        }
        let shown: Vec<&Process> = processes.into_iter().take(PROCESS_ROWS).collect();
        let top_cpu = shown.iter().map(|p| p.cpu).fold(0.0_f32, f32::max).max(f32::EPSILON);
        let top_memory = shown.iter().map(|p| p.memory).max().unwrap_or(1).max(1) as f32;
        let bar = |share: f32| (share * 100.0).round().clamp(0.0, 100.0) as u8;
        let rows = shown
            .into_iter()
            .map(|p| ProcessRow {
                pid: p.pid,
                name: p.name.clone(),
                command: if p.command.is_empty() { p.name.clone() } else { p.command.clone() },
                user: p.user.clone(),
                cpu: Meter::new(p.cpu),
                cpu_text: format!("{:.1}%", p.cpu),
                memory: bytes(p.memory),
                memory_meter: Meter::new(Usage { used: p.memory, total: s.memory.total }.percent()),
                cpu_bar: bar(p.cpu / top_cpu),
                memory_bar: bar(p.memory as f32 / top_memory),
                own: p.own,
            })
            .collect();
        Self { by_memory: sort == ProcessSort::Memory, rows }
    }
}

/// Which services the list shows, from the filter form.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServiceFilter {
    #[serde(default)]
    pub scope: ScopeFilter,
    #[serde(default)]
    pub state: StateFilter,
    #[serde(default)]
    pub q: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeFilter {
    #[default]
    All,
    System,
    User,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StateFilter {
    All,
    #[default]
    Running,
    Failed,
    Stopped,
}

impl ServiceFilter {
    fn keeps(&self, s: &Service) -> bool {
        let state = match self.state {
            StateFilter::All => true,
            StateFilter::Running => s.is_running(),
            StateFilter::Failed => s.is_failing(),
            StateFilter::Stopped => s.active == "inactive",
        };
        let q = self.q.trim().to_lowercase();
        state
            && (q.is_empty()
                || s.name.as_str().to_lowercase().contains(&q)
                || s.description.to_lowercase().contains(&q))
    }
}

/// The polled services list.
#[derive(Template)]
#[template(path = "services.html")]
pub struct ServiceList {
    pub rows: Vec<ServiceRow>,
    pub summary: String,
    pub errors: Vec<String>,
}

pub struct ServiceRow {
    pub url: String,
    pub name: String,
    pub description: String,
    pub user: bool,
    pub state: &'static str,
    pub state_label: String,
    pub boot: String,
    pub memory: String,
}

impl ServiceList {
    pub fn new(services: Vec<Service>, filter: &ServiceFilter, errors: Vec<String>) -> Self {
        let running = services.iter().filter(|s| s.is_running()).count();
        let failing = services.iter().filter(|s| s.is_failing()).count();
        let mut summary = format!("{running} running of {}", services.len());
        if failing > 0 {
            summary.push_str(&format!(" · {failing} failing"));
        }
        let rows = services.iter().filter(|s| filter.keeps(s)).map(ServiceRow::from).collect();
        Self { rows, summary, errors }
    }
}

impl From<&Service> for ServiceRow {
    fn from(s: &Service) -> Self {
        Self {
            url: format!("services/{}/{}", s.scope, s.name),
            name: s.name.as_str().trim_end_matches(".service").to_owned(),
            description: s.description.clone(),
            user: s.scope == Scope::User,
            state: state_class(s),
            state_label: if s.crash_looping { "crash-looping".to_owned() } else { s.sub.clone() },
            boot: s.file_state.clone().unwrap_or_default(),
            memory: s.memory.map(bytes).unwrap_or_default(),
        }
    }
}

/// One service's detail sheet.
#[derive(Template)]
#[template(path = "service.html")]
pub struct ServiceSheet {
    pub url: String,
    pub name: String,
    pub description: String,
    pub state: &'static str,
    pub state_label: String,
    pub facts: Vec<(&'static str, String)>,
    pub unit_file: Option<String>,
    pub running: bool,
    pub masked: bool,
    pub can_enable: bool,
    pub can_disable: bool,
    /// Extra words for the confirmation when stopping it could end remote access.
    pub warning: String,
    /// The confirmation for Delete, saying exactly what will happen; `None` hides Delete.
    pub delete_confirm: Option<String>,
    pub logs: String,
}

impl ServiceSheet {
    pub fn new(d: Details, logs: Result<String, String>, delete: Option<DeleteMode>) -> Self {
        let s = &d.service;
        let mut facts = vec![("Scope", if s.scope == Scope::User { "User" } else { "System" }.to_owned())];
        if let Some(boot) = &s.file_state {
            facts.push(("Starts at boot", boot.clone()));
        }
        if let Some(since) = d.active_since.filter(|_| s.active == "active") {
            facts.push(("Running for", duration(now_secs().saturating_sub(since))));
        }
        if let Some(memory) = s.memory {
            facts.push(("Memory", bytes(memory)));
        }
        if let Some(nsec) = d.cpu_nsec {
            facts.push(("CPU time", cpu_time(nsec)));
        }
        if let Some(n) = d.restarts {
            facts.push(("Restarts", n.to_string()));
        }
        if let Some(tasks) = d.tasks {
            facts.push(("Tasks", tasks.to_string()));
        }
        if let Some(pid) = d.main_pid {
            facts.push(("Main PID", pid.to_string()));
        }
        let warning = if s.name.is_lifeline() {
            " This service keeps the machine reachable; stopping it can end your remote session.".to_owned()
        } else {
            String::new()
        };
        let running = matches!(s.active.as_str(), "active" | "activating" | "reloading");
        let masked = s.file_state.as_deref().is_some_and(|f| f.starts_with("masked"));
        let delete_confirm = delete.filter(|_| !masked).map(|mode| {
            let what = match mode {
                DeleteMode::RemoveFile(path) => {
                    format!("It is stopped, disabled and its unit file {} is removed. This can't be undone.", path.display())
                }
                DeleteMode::Mask => "It is stopped and masked: systemd refuses to start it until you unmask it. Its files stay on disk.".to_owned(),
                DeleteMode::StopAndDisable(path) => format!(
                    "Its unit file {} belongs to root, so it is stopped and disabled, and you get the command to remove the file.",
                    path.display()
                ),
            };
            format!("Delete {}? {what}{warning}", s.name)
        });
        let logs = logs.unwrap_or_else(|e| e);
        Self {
            url: format!("services/{}/{}", s.scope, s.name),
            name: s.name.to_string(),
            description: s.description.clone(),
            state: state_class(s),
            state_label: if s.crash_looping {
                "crash-looping: keeps exiting and being restarted".to_owned()
            } else {
                format!("{} ({})", s.active, s.sub)
            },
            facts,
            unit_file: d.fragment_path.clone(),
            running,
            masked,
            delete_confirm,
            can_enable: s.file_state.as_deref() == Some("disabled"),
            can_disable: s.file_state.as_deref() == Some("enabled"),
            warning,
            logs: if logs.trim().is_empty() { "No log entries.".to_owned() } else { logs },
        }
    }
}

/// The agents panel.
#[derive(Template)]
#[template(path = "agents.html")]
pub struct AgentPane {
    pub summary: String,
    pub herdr_error: Option<String>,
    pub rows: Vec<AgentRow>,
}

pub struct AgentRow {
    /// herdr's state or `running`; styles the row.
    pub status: String,
    pub label: &'static str,
    pub title: String,
    pub kind: String,
    /// Workspace and folder, e.g. `backend · ~/projects/api`.
    pub place: String,
    pub cpu: String,
    pub memory: String,
    pub age: String,
}

impl AgentPane {
    pub fn new(list: AgentList, home: &Path) -> Self {
        let count = |status: &str| list.agents.iter().filter(|a| a.status == status).count();
        let mut summary = format!("{} {}", list.agents.len(), if list.agents.len() == 1 { "agent" } else { "agents" });
        let (working, waiting) = (count("working"), count("blocked"));
        if working > 0 {
            summary.push_str(&format!(" · {working} working"));
        }
        if waiting > 0 {
            summary.push_str(&format!(" · {waiting} waiting for you"));
        }
        let rows = list
            .agents
            .into_iter()
            .map(|a| {
                let folder = if a.cwd.is_empty() { String::new() } else { shown(Path::new(&a.cwd), home) };
                AgentRow {
                    label: agent_label(&a.status),
                    place: match (&a.workspace, folder.is_empty()) {
                        (Some(w), false) => format!("{w} · {folder}"),
                        (Some(w), true) => w.clone(),
                        (None, false) => format!("outside herdr · {folder}"),
                        (None, true) => "outside herdr".to_owned(),
                    },
                    cpu: a.cpu.map(|c| format!("{c:.1}%")).unwrap_or_default(),
                    memory: a.memory.map(bytes).unwrap_or_default(),
                    age: a.run_secs.map(duration).unwrap_or_default(),
                    title: a.title,
                    kind: a.kind,
                    status: a.status,
                }
            })
            .collect();
        Self { summary, herdr_error: list.herdr_error, rows }
    }
}

/// Words for an agent's state.
pub fn agent_label(status: &str) -> &'static str {
    match status {
        "blocked" => "Waiting for you",
        "working" => "Working",
        "done" => "Done",
        "idle" => "Idle",
        "running" => "Running",
        _ => "Unknown",
    }
}

/// The storage browser: one folder of the disk-usage index, or the scan in progress.
#[derive(Template)]
#[template(path = "storage.html")]
pub struct StoragePane {
    /// This pane's own URL, which it polls while scanning and reloads after changes.
    pub url: String,
    pub scanning: Option<String>,
    pub missing: Option<String>,
    pub scanned: Option<String>,
    pub crumbs: Vec<Crumb>,
    pub total: String,
    pub summary: String,
    pub rows: Vec<StorageRow>,
    pub more: Option<String>,
}

pub struct Crumb {
    pub label: String,
    pub url: String,
}

pub struct StorageRow {
    pub name: String,
    pub is_dir: bool,
    /// Where tapping the row goes; folders only.
    pub url: Option<String>,
    pub size: String,
    pub detail: String,
    /// Share of the current folder, 0–100.
    pub share: u8,
    /// `hx-vals` JSON and the confirmation, when the row may be deleted.
    pub delete: Option<(String, String)>,
}

/// Top-level folders in home whose loss breaks logins, keys or every program's settings.
const PRECIOUS: [&str; 6] = [".ssh", ".gnupg", ".config", ".local", ".password-store", ".pki"];

impl StoragePane {
    pub fn new(listing: Listing, storage: &Storage) -> Self {
        let home = storage.home();
        let mut pane = Self {
            url: storage_url(Path::new("/")),
            scanning: None,
            missing: None,
            scanned: None,
            crumbs: Vec::new(),
            total: String::new(),
            summary: String::new(),
            rows: Vec::new(),
            more: None,
        };
        match listing {
            Listing::Scanning { dirs, bytes } => {
                pane.scanning = Some(format!("{} folders · {} so far", count(dirs), self::bytes(bytes)));
            }
            Listing::NotIndexed { path } => {
                pane.url = storage_url(&path);
                pane.missing = Some(format!(
                    "{} isn't in the last scan: it is new, or it couldn't be read. Rescan to include it.",
                    shown(&path, home)
                ));
            }
            Listing::Folder(f) => {
                pane.url = storage_url(&f.path);
                let ago = SystemTime::now().duration_since(f.scanned).unwrap_or_default().as_secs();
                pane.scanned = Some(format!("Scanned {} ago in {}", duration(ago), duration(f.took.as_secs())));
                pane.crumbs = crumbs(&f.path);
                pane.total = bytes(f.size);
                pane.summary = if f.unreadable {
                    "in this folder · you can't read inside it".to_owned()
                } else {
                    format!("in {}", files(f.files))
                };
                if f.path == Path::new("/") && f.unreadable_dirs > 0 {
                    pane.summary.push_str(&format!(" · {} folders unreadable as you", count(f.unreadable_dirs)));
                }
                let total = f.size.max(1) as f64;
                pane.rows = f
                    .entries
                    .into_iter()
                    .map(|e| {
                        let delete = storage.deletable(&e.path).then(|| {
                            let vals = serde_json::json!({ "path": e.path.to_string_lossy() }).to_string();
                            let what = match e.files {
                                Some(n) => format!("{}, {}", bytes(e.size), files(n)),
                                None => bytes(e.size),
                            };
                            let precious = e
                                .path
                                .strip_prefix(home)
                                .ok()
                                .and_then(|rest| rest.components().next())
                                .is_some_and(|top| PRECIOUS.iter().any(|p| top.as_os_str() == *p));
                            let warning = if precious {
                                " It holds keys or settings that logins and programs depend on."
                            } else {
                                ""
                            };
                            (vals, format!("Delete {} permanently? {what}. This can't be undone.{warning}", shown(&e.path, home)))
                        });
                        StorageRow {
                            url: (e.is_dir && !e.unreadable).then(|| storage_url(&e.path)),
                            detail: match (e.files, e.unreadable) {
                                (_, true) => "can't read inside".to_owned(),
                                (Some(n), false) => files(n),
                                (None, false) => "file".to_owned(),
                            },
                            share: ((e.size as f64 / total) * 100.0).round().clamp(0.0, 100.0) as u8,
                            size: bytes(e.size),
                            is_dir: e.is_dir,
                            name: e.name,
                            delete,
                        }
                    })
                    .collect();
                if f.more > 0 {
                    pane.more = Some(format!("{} smaller items · {}", count(f.more as u64), bytes(f.more_size)));
                }
            }
        }
        pane
    }
}

/// The pane URL for a folder, percent-encoded so any file name survives the query string.
fn storage_url(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut out = String::from("storage?path=");
    for b in raw.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `~/projects` for paths in home, the full path otherwise.
fn shown(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// One crumb per folder from the root down.
fn crumbs(path: &Path) -> Vec<Crumb> {
    let mut at = PathBuf::from("/");
    let mut crumbs = vec![Crumb { label: "/".to_owned(), url: storage_url(&at) }];
    for part in path.components().skip(1) {
        at.push(part);
        crumbs.push(Crumb { label: part.as_os_str().to_string_lossy().into_owned(), url: storage_url(&at) });
    }
    crumbs
}

/// `1 file`, `41,203 files`.
fn files(n: u64) -> String {
    if n == 1 { "1 file".to_owned() } else { format!("{} files", count(n)) }
}

/// Thousands separated: `41,203`.
fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn state_class(s: &Service) -> &'static str {
    if s.is_failing() {
        return "failed";
    }
    match s.active.as_str() {
        "active" if s.is_running() => "running",
        "active" => "done",
        "reloading" => "running",
        "activating" | "deactivating" => "changing",
        _ => "stopped",
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

/// Binary units, one decimal below 10: `512 KiB`, `3.4 GiB`, `12 GiB`.
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value >= 10.0 { format!("{value:.0} {}", UNITS[unit]) } else { format!("{value:.1} {}", UNITS[unit]) }
}

fn rate(bytes_per_sec: u64) -> String {
    format!("{}/s", bytes(bytes_per_sec))
}

/// `174 / 237 GiB`, or `62 MiB / 1.0 GiB` when the units differ.
fn usage(u: Usage) -> String {
    let (used, total) = (bytes(u.used), bytes(u.total));
    match (used.rsplit_once(' '), total.rsplit_once(' ')) {
        (Some((n, a)), Some((_, b))) if a == b => format!("{n} / {total}"),
        _ => format!("{used} / {total}"),
    }
}

/// The two largest units: `3d 4h`, `5h 12m`, `12m`, `45s`.
pub fn duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs / 3_600 % 24, secs / 60 % 60);
    match (d, h, m) {
        (0, 0, 0) => format!("{secs}s"),
        (0, 0, m) => format!("{m}m"),
        (0, h, m) => format!("{h}h {m}m"),
        (d, h, _) => format!("{d}d {h}h"),
    }
}

fn cpu_time(nsec: u64) -> String {
    let secs = nsec / 1_000_000_000;
    if secs < 60 { format!("{:.1}s", nsec as f64 / 1e9) } else { duration(secs) }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Usage, bytes, count, duration, storage_url, usage};

    #[test]
    fn counts_and_urls() {
        assert_eq!(count(41203), "41,203");
        assert_eq!(count(999), "999");
        assert_eq!(storage_url(Path::new("/home/u/My Files&x")), "storage?path=/home/u/My%20Files%26x");
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1536), "1.5 KiB");
        assert_eq!(bytes(12 * 1024 * 1024 * 1024), "12 GiB");
    }

    #[test]
    fn usage_shares_the_unit_when_it_can() {
        const GIB: u64 = 1024 * 1024 * 1024;
        assert_eq!(usage(Usage { used: 174 * GIB, total: 237 * GIB }), "174 / 237 GiB");
        assert_eq!(usage(Usage { used: 62 * 1024 * 1024, total: GIB }), "62 MiB / 1.0 GiB");
    }

    #[test]
    fn formats_durations() {
        assert_eq!(duration(45), "45s");
        assert_eq!(duration(12 * 60 + 5), "12m");
        assert_eq!(duration(5 * 3600 + 12 * 60), "5h 12m");
        assert_eq!(duration(3 * 86_400 + 4 * 3600 + 59), "3d 4h");
    }
}
