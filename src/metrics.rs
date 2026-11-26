//! System stats: a background task samples CPU, memory, disks, network, temperatures and
//! processes every [`SAMPLE_EVERY`] and publishes an immutable [`Snapshot`]. Requests read the
//! latest snapshot, so any number of open dashboards costs one sample per tick.

use std::{collections::VecDeque, sync::Arc, time::Duration};

use sysinfo::{
    Components, CpuRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind, ProcessesToUpdate,
    RefreshKind, System, UpdateKind, Users,
};
use tokio::sync::watch;

pub const SAMPLE_EVERY: Duration = Duration::from_secs(2);
/// Points kept for the CPU and memory charts: two minutes at the sample rate.
const HISTORY: usize = 60;
/// Processes kept per snapshot, enough for both the "by CPU" and "by memory" lists.
const TOP_PROCESSES: usize = 40;
/// Interfaces that only carry traffic between local containers, not the machine's own.
const VIRTUAL_IFACES: [&str; 4] = ["lo", "docker", "br-", "veth"];
/// Filesystems that aren't real disks.
const PSEUDO_FS: [&str; 6] = ["tmpfs", "devtmpfs", "overlay", "squashfs", "efivarfs", "ramfs"];

/// One sample of the whole machine.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub host: Host,
    /// Overall CPU use, 0–100.
    pub cpu: f32,
    /// Per logical core, 0–100.
    pub cores: Vec<f32>,
    pub cpu_history: Vec<f32>,
    pub load: [f64; 3],
    pub memory: Usage,
    /// RAM in use as a percentage, for the memory chart.
    pub memory_history: Vec<f32>,
    pub swap: Usage,
    pub disks: Vec<Disk>,
    pub networks: Vec<Network>,
    pub temperatures: Vec<Temperature>,
    pub processes: Vec<Process>,
    /// Every running AI coding agent process, with the load of everything it spawned.
    pub agents: Vec<AgentProcess>,
    /// The machine's own battery; `None` on a machine without one.
    pub battery: Option<Battery>,
    pub uptime_secs: u64,
}

/// Facts that don't change while the dashboard runs.
#[derive(Debug, Clone, Default)]
pub struct Host {
    pub name: String,
    pub os: String,
    pub kernel: String,
    pub cpu_brand: String,
    pub physical_cores: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Usage {
    pub used: u64,
    pub total: u64,
}

impl Usage {
    pub fn percent(&self) -> f32 {
        if self.total == 0 { 0.0 } else { self.used as f32 / self.total as f32 * 100.0 }
    }
}

#[derive(Debug, Clone)]
pub struct Disk {
    pub mount: String,
    pub fs: String,
    pub space: Usage,
}

#[derive(Debug, Clone)]
pub struct Network {
    pub name: String,
    /// Bytes per second over the last sample.
    pub rx_rate: u64,
    pub tx_rate: u64,
}

#[derive(Debug, Clone)]
pub struct Temperature {
    pub label: String,
    pub celsius: f32,
}

/// The machine's battery, read from its power supply's `uevent` in sysfs.
#[derive(Debug, Clone, PartialEq)]
pub struct Battery {
    /// Charge, 0–100.
    pub percent: f32,
    pub state: Charge,
    /// Time until empty while discharging, or until full while charging, at the current rate.
    pub remaining_secs: Option<u64>,
    /// Power flowing in or out, in watts.
    pub watts: Option<f32>,
    /// What a full charge holds now, as a share of what it held new, 0–100.
    pub health: Option<f32>,
    pub cycles: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charge {
    Charging,
    Discharging,
    Full,
    /// Plugged in but held where it is, e.g. by a charge limit.
    NotCharging,
}

/// Where the kernel lists power supplies: batteries, chargers, and peripherals' batteries.
const POWER_SUPPLIES: &str = "/sys/class/power_supply";
/// Longer estimates come from a near-zero rate and mean nothing.
const MAX_REMAINING_SECS: f64 = 48.0 * 3600.0;

impl Battery {
    /// The first system battery under [`POWER_SUPPLIES`].
    fn read() -> Option<Self> {
        std::fs::read_dir(POWER_SUPPLIES)
            .ok()?
            .flatten()
            .find_map(|e| Self::from_uevent(&std::fs::read_to_string(e.path().join("uevent")).ok()?))
    }

    /// Parses a power supply's `uevent`. `None` unless it's a battery that powers the machine,
    /// not a wireless mouse's (scope `Device`). Batteries report either energy (µWh, µW) or
    /// charge (µAh, µA); the arithmetic is the same in either unit.
    fn from_uevent(uevent: &str) -> Option<Self> {
        let field = |key: &str| {
            uevent.lines().find_map(|l| l.strip_prefix("POWER_SUPPLY_")?.strip_prefix(key)?.strip_prefix('='))
        };
        let num = |key: &str| field(key)?.trim().parse::<f64>().ok();
        if field("TYPE")? != "Battery" || field("SCOPE") == Some("Device") || field("PRESENT") == Some("0") {
            return None;
        }
        let unit = if num("ENERGY_NOW").is_some() { "ENERGY" } else { "CHARGE" };
        let now = num(&format!("{unit}_NOW"));
        let full = num(&format!("{unit}_FULL")).filter(|f| *f > 0.0);
        let design = num(&format!("{unit}_FULL_DESIGN")).filter(|d| *d > 0.0);
        let percent = num("CAPACITY").or_else(|| Some(now? / full? * 100.0))?;
        let state = match field("STATUS") {
            Some("Charging") => Charge::Charging,
            Some("Discharging") => Charge::Discharging,
            Some("Full") => Charge::Full,
            _ => Charge::NotCharging,
        };
        // Some batteries report the rate negative while discharging.
        let rate = if unit == "ENERGY" { num("POWER_NOW") } else { num("CURRENT_NOW") }.map(f64::abs).filter(|r| *r > 0.0);
        let watts = match unit {
            "ENERGY" => rate.map(|uw| uw / 1e6),
            _ => rate.zip(num("VOLTAGE_NOW")).map(|(ua, uv)| ua * uv / 1e12),
        };
        let left = match state {
            Charge::Discharging => now,
            Charge::Charging => full.zip(now).map(|(f, n)| (f - n).max(0.0)),
            Charge::Full | Charge::NotCharging => None,
        };
        let remaining_secs =
            left.zip(rate).map(|(l, r)| l / r * 3600.0).filter(|s| *s <= MAX_REMAINING_SECS).map(|s| s as u64);
        Some(Self {
            percent: percent.clamp(0.0, 100.0) as f32,
            state,
            remaining_secs,
            watts: watts.map(|w| w as f32),
            health: full.zip(design).map(|(f, d)| (f / d * 100.0) as f32),
            cycles: num("CYCLE_COUNT").filter(|c| *c > 0.0).map(|c| c as u32),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AgentProcess {
    pub pid: u32,
    /// The agent's name as matched: `claude`, `codex`, …
    pub kind: String,
    pub command: String,
    pub cwd: String,
    /// Share of the whole machine, 0–100, the agent and all its descendants together.
    pub cpu: f32,
    pub memory: u64,
    pub run_secs: u64,
}

#[derive(Debug, Clone)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    pub command: String,
    pub user: String,
    /// Share of the whole machine, 0–100 (sysinfo reports per-core, up to cores × 100).
    pub cpu: f32,
    pub memory: u64,
    /// Whether the dashboard's own user owns it, i.e. may signal it.
    pub own: bool,
}

/// Starts the sampler and returns a receiver that always holds the latest snapshot.
pub fn spawn() -> watch::Receiver<Arc<Snapshot>> {
    let (tx, rx) = watch::channel(Arc::new(Snapshot::default()));
    tokio::task::spawn_blocking(move || Sampler::new().run(&tx));
    rx
}

/// Owns the sysinfo handles. Runs on a blocking thread because refreshing reads hundreds of
/// files under /proc.
struct Sampler {
    system: System,
    disks: Disks,
    networks: Networks,
    components: Components,
    users: Users,
    host: Host,
    own_uid: u32,
    cpu_history: VecDeque<f32>,
    memory_history: VecDeque<f32>,
}

impl Sampler {
    fn new() -> Self {
        let refresh = RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing().with_usage())
            .with_memory(MemoryRefreshKind::everything());
        let system = System::new_with_specifics(refresh).expect("sysinfo supports Linux");
        let host = Host {
            name: System::host_name().unwrap_or_default(),
            os: System::name().unwrap_or_default(),
            kernel: System::kernel_version().unwrap_or_default(),
            cpu_brand: system.cpus().first().map(|c| c.brand().trim().to_owned()).unwrap_or_default(),
            physical_cores: System::physical_core_count().unwrap_or_default(),
        };
        Self {
            system,
            disks: Disks::new_with_refreshed_list().expect("sysinfo supports Linux"),
            networks: Networks::new_with_refreshed_list().expect("sysinfo supports Linux"),
            components: Components::new_with_refreshed_list().expect("sysinfo supports Linux"),
            users: Users::new_with_refreshed_list().expect("sysinfo supports Linux"),
            host,
            own_uid: own_uid(),
            cpu_history: VecDeque::with_capacity(HISTORY),
            memory_history: VecDeque::with_capacity(HISTORY),
        }
    }

    /// Samples forever, until every receiver is gone.
    fn run(mut self, tx: &watch::Sender<Arc<Snapshot>>) {
        // CPU use is a difference between two readings, so prime the first one.
        self.refresh();
        loop {
            std::thread::sleep(SAMPLE_EVERY);
            self.refresh();
            if tx.send(Arc::new(self.snapshot())).is_err() {
                return;
            }
        }
    }

    fn refresh(&mut self) {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        let processes = ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .with_user(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_cwd(UpdateKind::OnlyIfNotSet);
        self.system.refresh_processes_specifics(ProcessesToUpdate::All, true, processes);
        self.disks.refresh(true);
        self.networks.refresh(true);
        self.components.refresh(false);
    }

    fn snapshot(&mut self) -> Snapshot {
        let cpu = self.system.global_cpu_usage();
        let memory = Usage { used: self.system.used_memory(), total: self.system.total_memory() };
        push_bounded(&mut self.cpu_history, cpu);
        push_bounded(&mut self.memory_history, memory.percent());
        let load = System::load_average().map(|l| [l.one, l.five, l.fifteen]).unwrap_or_default();

        Snapshot {
            host: self.host.clone(),
            cpu,
            cores: self.system.cpus().iter().map(|c| c.usage()).collect(),
            cpu_history: self.cpu_history.iter().copied().collect(),
            load,
            memory,
            memory_history: self.memory_history.iter().copied().collect(),
            swap: Usage { used: self.system.used_swap(), total: self.system.total_swap() },
            disks: self.disks(),
            networks: self.networks(),
            temperatures: self.temperatures(),
            processes: self.processes(),
            agents: self.agents(),
            battery: Battery::read(),
            uptime_secs: System::uptime().unwrap_or_default(),
        }
    }

    /// Real filesystems, one row per device (btrfs mounts the same device at several subvolumes).
    fn disks(&self) -> Vec<Disk> {
        let mut seen = Vec::new();
        let mut disks: Vec<Disk> = self
            .disks
            .iter()
            .filter(|d| !PSEUDO_FS.contains(&d.file_system().to_string_lossy().as_ref()) && d.total_space() > 0)
            .filter(|d| {
                let device = d.name().to_owned();
                let new = !seen.contains(&device);
                seen.push(device);
                new
            })
            .map(|d| Disk {
                mount: d.mount_point().display().to_string(),
                fs: d.file_system().to_string_lossy().into_owned(),
                space: Usage { used: d.total_space() - d.available_space(), total: d.total_space() },
            })
            .collect();
        disks.sort_by(|a, b| a.mount.cmp(&b.mount));
        disks
    }

    fn networks(&self) -> Vec<Network> {
        let secs = SAMPLE_EVERY.as_secs().max(1);
        let mut networks: Vec<Network> = self
            .networks
            .iter()
            .map(|(name, data)| (name.to_string_lossy().into_owned(), data))
            .filter(|(name, _)| !VIRTUAL_IFACES.iter().any(|v| name.starts_with(v)))
            .map(|(name, data)| Network {
                name,
                rx_rate: data.received() / secs,
                tx_rate: data.transmitted() / secs,
            })
            .collect();
        networks.sort_by(|a, b| a.name.cmp(&b.name));
        networks
    }

    /// Hottest first.
    fn temperatures(&self) -> Vec<Temperature> {
        let mut temps: Vec<Temperature> = self
            .components
            .iter()
            .filter_map(|c| Some(Temperature { label: c.label().to_owned(), celsius: c.temperature()? }))
            .filter(|t| t.celsius.is_finite() && t.celsius > 0.0)
            .collect();
        temps.sort_by(|a, b| b.celsius.total_cmp(&a.celsius));
        temps
    }

    /// The top processes by CPU merged with the top by memory, so either list can be shown.
    fn processes(&self) -> Vec<Process> {
        let cores = self.system.cpus().len().max(1) as f32;
        let mut all: Vec<Process> = self
            .system
            .processes()
            .values()
            // Threads show up as their own entries on Linux; count each process once.
            .filter(|p| p.thread_kind().is_none())
            .map(|p| {
                let uid = p.user_id().map(|u| **u);
                let user = p
                    .user_id()
                    .and_then(|u| self.users.get_user_by_id(u))
                    .map_or_else(|| uid.map(|u| u.to_string()).unwrap_or_default(), |u| u.name().to_owned());
                let command = p.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ");
                Process {
                    pid: p.pid().as_u32(),
                    name: display_name(p),
                    command,
                    user,
                    cpu: p.cpu_usage() / cores,
                    memory: p.memory(),
                    own: uid == Some(self.own_uid),
                }
            })
            .collect();

        all.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
        let mut top: Vec<Process> = all.iter().take(TOP_PROCESSES / 2).cloned().collect();
        all.sort_by_key(|p| std::cmp::Reverse(p.memory));
        for p in all.into_iter().take(TOP_PROCESSES / 2) {
            if !top.iter().any(|t| t.pid == p.pid) {
                top.push(p);
            }
        }
        top
    }
}

/// AI coding agents, by process name or by the script a runtime (node, python) runs.
const AGENT_NAMES: [&str; 13] = [
    "claude", "codex", "opencode", "gemini", "aider", "goose", "amp", "crush", "droid", "cursor-agent", "qwen",
    "kimi", "copilot",
];

impl Sampler {
    /// Agent processes with their own and their descendants' CPU and memory. An agent started
    /// by another agent counts toward its parent, not as a second agent.
    fn agents(&self) -> Vec<AgentProcess> {
        let cores = self.system.cpus().len().max(1) as f32;
        let processes = self.system.processes();
        let mut children: std::collections::HashMap<sysinfo::Pid, Vec<sysinfo::Pid>> = Default::default();
        for (pid, p) in processes {
            if p.thread_kind().is_none()
                && let Some(parent) = p.parent()
            {
                children.entry(parent).or_default().push(*pid);
            }
        }
        let kind = |p: &sysinfo::Process| agent_kind(p);
        let mut agents = Vec::new();
        for (pid, p) in processes {
            if p.thread_kind().is_some() {
                continue;
            }
            let Some(name) = kind(p) else { continue };
            // Skip agents nested under another agent.
            let mut up = p.parent();
            let mut nested = false;
            while let Some(parent) = up.and_then(|pp| processes.get(&pp)) {
                if kind(parent).is_some() {
                    nested = true;
                    break;
                }
                up = parent.parent();
            }
            if nested {
                continue;
            }
            let (mut cpu, mut memory) = (0.0, 0);
            let mut stack = vec![*pid];
            while let Some(at) = stack.pop() {
                if let Some(q) = processes.get(&at) {
                    cpu += q.cpu_usage();
                    memory += q.memory();
                }
                stack.extend(children.get(&at).into_iter().flatten().copied());
            }
            agents.push(AgentProcess {
                pid: pid.as_u32(),
                kind: name.to_owned(),
                command: p.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" "),
                cwd: p.cwd().map(|c| c.display().to_string()).unwrap_or_default(),
                cpu: cpu / cores,
                memory,
                run_secs: p.run_time(),
            });
        }
        agents.sort_by_key(|a| a.pid);
        agents
    }
}

/// The agent a process is, if any: its own name, or the script its runtime was started with.
fn agent_kind(p: &sysinfo::Process) -> Option<&'static str> {
    let name = p.name().to_string_lossy();
    if let Some(found) = AGENT_NAMES.iter().find(|a| **a == name) {
        return Some(found);
    }
    if !matches!(name.as_ref(), "node" | "bun" | "python" | "python3" | "deno") {
        return None;
    }
    let script = p.cmd().get(1)?.to_string_lossy();
    let base = script.rsplit('/').next().unwrap_or_default();
    AGENT_NAMES.iter().find(|a| base == **a || base.strip_suffix(".js") == Some(**a)).copied()
}

/// The kernel cuts process names at 15 bytes ("Plex Media Serv"); use the executable's full
/// name from the command line when it starts with the cut one.
fn display_name(p: &sysinfo::Process) -> String {
    let name = p.name().to_string_lossy();
    if name.len() < 15 {
        return name.into_owned();
    }
    let program = p.cmd().first().map(|a| a.to_string_lossy()).unwrap_or_default();
    let base = program.rsplit('/').next().unwrap_or_default();
    if base.starts_with(name.as_ref()) { base.to_owned() } else { name.into_owned() }
}

fn push_bounded(history: &mut VecDeque<f32>, value: f32) {
    if history.len() == HISTORY {
        history.pop_front();
    }
    history.push_back(value);
}

/// The uid this server runs as, read from /proc so no libc call is needed.
pub fn own_uid() -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAPTOP: &str = "POWER_SUPPLY_NAME=BAT0
POWER_SUPPLY_TYPE=Battery
POWER_SUPPLY_STATUS=Discharging
POWER_SUPPLY_CHARGE_TYPES=Long Life
POWER_SUPPLY_PRESENT=1
POWER_SUPPLY_CYCLE_COUNT=383
POWER_SUPPLY_VOLTAGE_NOW=11357000
POWER_SUPPLY_POWER_NOW=5212000
POWER_SUPPLY_ENERGY_FULL_DESIGN=52650000
POWER_SUPPLY_ENERGY_FULL=42850000
POWER_SUPPLY_ENERGY_NOW=18670000
POWER_SUPPLY_CAPACITY=44
";

    #[test]
    fn reads_an_energy_battery() {
        let b = Battery::from_uevent(LAPTOP).unwrap();
        assert_eq!((b.percent, b.state, b.cycles), (44.0, Charge::Discharging, Some(383)));
        // 18.67 Wh at 5.212 W.
        assert_eq!(b.remaining_secs, Some(12_895));
        assert_eq!(b.watts.map(|w| (w * 10.0).round()), Some(52.0));
        assert_eq!(b.health.map(f32::round), Some(81.0));
    }

    #[test]
    fn reads_a_charge_battery_charging() {
        let uevent = "POWER_SUPPLY_TYPE=Battery
POWER_SUPPLY_STATUS=Charging
POWER_SUPPLY_CHARGE_FULL=4000000
POWER_SUPPLY_CHARGE_NOW=3000000
POWER_SUPPLY_CURRENT_NOW=-2000000
POWER_SUPPLY_VOLTAGE_NOW=12000000
";
        let b = Battery::from_uevent(uevent).unwrap();
        // No CAPACITY: worked out from the charge. 1 Ah to go at 2 A.
        assert_eq!((b.percent, b.state, b.remaining_secs), (75.0, Charge::Charging, Some(1800)));
        assert_eq!(b.watts, Some(24.0));
        assert_eq!(b.health, None);
    }

    #[test]
    fn skips_chargers_and_peripheral_batteries() {
        assert_eq!(Battery::from_uevent("POWER_SUPPLY_TYPE=Mains\nPOWER_SUPPLY_ONLINE=1\n"), None);
        let mouse = "POWER_SUPPLY_TYPE=Battery\nPOWER_SUPPLY_SCOPE=Device\nPOWER_SUPPLY_CAPACITY=80\n";
        assert_eq!(Battery::from_uevent(mouse), None);
    }

    #[test]
    fn has_no_estimate_when_plugged_in_and_holding() {
        let held = LAPTOP.replace("Discharging", "Not charging");
        let b = Battery::from_uevent(&held).unwrap();
        assert_eq!((b.state, b.remaining_secs), (Charge::NotCharging, None));
    }
}
