//! Services through systemd's D-Bus API: the system manager for machine services and the user
//! manager for this user's own. Logs come from `journalctl`.
//!
//! Changing system services as a normal user needs polkit's permission; without the rule in
//! `deploy/50-sysdash.rules` those calls fail with [`SystemdError::NotAllowed`].

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Deserialize;
use tokio::process::Command;
use zbus::{Connection, OwnedObjectPath, OwnedVariant as OwnedValue};

const DEST: &str = "org.freedesktop.systemd1";
const PATH: &str = "/org/freedesktop/systemd1";
const MANAGER: &str = "org.freedesktop.systemd1.Manager";
/// How long unit-file states are reused. Reading them makes systemd scan every unit file on
/// disk (~0.5 s for the system manager), and they only change on enable/disable or installs.
const FILE_STATES_TTL: Duration = Duration::from_secs(60);
/// Automatic restarts after which a still-starting service counts as crash-looping.
const CRASH_LOOP: u32 = 3;
/// How long a unit seen crash-looping stays flagged. Between attempts it is briefly "active",
/// so without this memory it would flicker in and out of the failing list.
const LOOP_MEMORY: Duration = Duration::from_secs(30);
/// systemd reports "not tracked" counters as the maximum value.
const UNSET: u64 = u64::MAX;
/// Units whose loss would cut off remote access to the machine; actions on them get a warning.
const LIFELINES: [&str; 6] = ["sshd", "NetworkManager", "wpa_supplicant", "wg-quick@", "dbus", "systemd-"];

/// Which service manager a unit belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    System,
    User,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A service unit name such as `nginx.service` or `wg-quick@wg0.service`. Anything else is
/// refused, so a request can never name another unit type or smuggle a `journalctl` option.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct UnitName(String);

impl TryFrom<String> for UnitName {
    type Error = &'static str;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let stem = s.strip_suffix(".service").ok_or("not a service")?;
        let allowed = |c: char| c.is_ascii_alphanumeric() || ":_.@-\\".contains(c);
        let valid = !stem.is_empty() && s.len() <= 256 && !s.starts_with('-') && s.chars().all(allowed);
        if valid { Ok(Self(s)) } else { Err("not a valid unit name") }
    }
}

impl UnitName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether stopping this unit could lock the owner out of a remote session.
    pub fn is_lifeline(&self) -> bool {
        LIFELINES.iter().any(|l| self.0.starts_with(l))
    }
}

impl fmt::Display for UnitName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Start,
    Stop,
    Restart,
    Enable,
    Disable,
    /// SIGKILL to every process of the unit.
    Kill,
    /// See [`DeleteMode`].
    Delete,
    Unmask,
}

/// What "Delete" does to a unit. systemd has no delete; this picks the strongest step the
/// dashboard can take without root or destroying a file it can't restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteMode {
    /// A user unit file the owner wrote: stop, disable and remove the file.
    RemoveFile(PathBuf),
    /// A packaged unit, or an instance of a template: stop and mask (a link to /dev/null in
    /// the admin directory blocks it until unmasked; files stay).
    Mask,
    /// A unit file in an admin directory that only root may change (`/etc/systemd/system`).
    /// Masking would have to overwrite it, so stop and disable it and hand back the command.
    StopAndDisable(PathBuf),
}

/// Picks the [`DeleteMode`] for a unit from where its file lives. `None` when there is no
/// unit file (generated or transient units), so there is nothing to delete.
pub fn delete_mode(scope: Scope, name: &UnitName, fragment: Option<&str>, user_unit_dir: &Path) -> Option<DeleteMode> {
    let fragment = Path::new(fragment.filter(|f| !f.is_empty())?);
    let instance = name.as_str().split_once('@').is_some_and(|(_, rest)| rest != "service");
    if fragment.starts_with("/run") {
        return None;
    }
    if instance {
        return Some(DeleteMode::Mask);
    }
    match scope {
        Scope::User if fragment.starts_with(user_unit_dir) => Some(DeleteMode::RemoveFile(fragment.to_owned())),
        Scope::System if fragment.starts_with("/etc") => Some(DeleteMode::StopAndDisable(fragment.to_owned())),
        _ => Some(DeleteMode::Mask),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SystemdError {
    #[error("The {0} service manager isn't reachable")]
    Unavailable(Scope),
    #[error("Not allowed. Managing system services needs the polkit rule from deploy/50-sysdash.rules")]
    NotAllowed,
    #[error("{0}")]
    Failed(String),
    #[error("Couldn't read the logs: {0}")]
    Logs(String),
}

impl From<zbus::Error> for SystemdError {
    fn from(e: zbus::Error) -> Self {
        match &e {
            // Standard org.freedesktop.DBus.Error.* replies, which is how polkit refusals arrive.
            zbus::Error::FDO(fdo) => match fdo.as_ref() {
                zbus::fdo::Error::InteractiveAuthorizationRequired(_) | zbus::fdo::Error::AccessDenied(_) => {
                    Self::NotAllowed
                }
                other => Self::Failed(other.to_string()),
            },
            zbus::Error::MethodError(name, message, _) => {
                let name = name.as_str();
                if name.ends_with("InteractiveAuthorizationRequired") || name.ends_with("AccessDenied") {
                    Self::NotAllowed
                } else {
                    Self::Failed(message.clone().unwrap_or_else(|| name.to_owned()))
                }
            }
            _ => Self::Failed(e.to_string()),
        }
    }
}

/// One service as listed: loaded units plus installed unit files that aren't loaded.
#[derive(Debug, Clone)]
pub struct Service {
    pub scope: Scope,
    pub name: UnitName,
    pub description: String,
    /// `active`, `inactive`, `failed`, `activating`, `deactivating`, `reloading`.
    pub active: String,
    /// The finer state, e.g. `running`, `exited`, `dead`.
    pub sub: String,
    /// `enabled`, `disabled`, `static`, `masked`, …; `None` for units without a unit file.
    pub file_state: Option<String>,
    pub memory: Option<u64>,
    /// Seen still starting after [`CRASH_LOOP`] or more automatic restarts, now or within
    /// [`LOOP_MEMORY`].
    pub crash_looping: bool,
}

impl Service {
    /// Has a live process (or is starting one). One-shot units that finished ("active
    /// (exited)") don't count, though systemd calls them active.
    pub fn is_running(&self) -> bool {
        !self.crash_looping && matches!(self.sub.as_str(), "running" | "start" | "start-pre" | "start-post" | "reload")
    }

    /// Failed, or crashing and waiting to be restarted again.
    pub fn is_failing(&self) -> bool {
        self.active == "failed" || self.crash_looping
    }
}

/// A service needing attention on the overview.
#[derive(Debug, Clone)]
pub struct Failing {
    pub scope: Scope,
    pub name: UnitName,
    /// Crash-looping (systemd keeps restarting it) rather than given up on.
    pub restarting: bool,
}

/// Everything shown on a service's detail sheet.
#[derive(Debug, Clone)]
pub struct Details {
    pub service: Service,
    pub main_pid: Option<u32>,
    /// Seconds since the Unix epoch at which it last became active.
    pub active_since: Option<u64>,
    pub cpu_nsec: Option<u64>,
    pub tasks: Option<u64>,
    /// Automatic restarts since the unit was loaded.
    pub restarts: Option<u32>,
    pub fragment_path: Option<String>,
}

/// Unit name → `enabled`/`disabled`/…, with when it was read.
type FileStates = (Instant, HashMap<String, String>);

/// `ListUnits` row: name, description, load, active, sub, following, path, job id, job type, job path.
type UnitRow = (String, String, String, String, String, String, OwnedObjectPath, u32, String, OwnedObjectPath);

/// Connections to both service managers. Either can be missing (no user session bus under a
/// system unit, say); its calls then fail with [`SystemdError::Unavailable`].
#[derive(Clone)]
pub struct Systemd {
    system: Option<Connection>,
    user: Option<Connection>,
    file_states: Arc<Mutex<HashMap<Scope, FileStates>>>,
    /// Units last seen crash-looping, and when.
    looping: Arc<Mutex<HashMap<(Scope, String), Instant>>>,
    /// Where this user's own unit files live (`$XDG_CONFIG_HOME/systemd/user`).
    user_unit_dir: PathBuf,
}

impl Systemd {
    pub async fn connect() -> Self {
        let system = Connection::system().await.inspect_err(|e| tracing::warn!("no system bus: {e}")).ok();
        let user = Connection::session().await.inspect_err(|e| tracing::warn!("no user bus: {e}")).ok();
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|| PathBuf::from("/nonexistent"));
        let user_unit_dir = config.join("systemd/user");
        Self { system, user, file_states: Default::default(), looping: Default::default(), user_unit_dir }
    }

    /// The [`DeleteMode`] for a unit given its unit file, for the detail sheet.
    pub fn delete_mode(&self, scope: Scope, name: &UnitName, fragment: Option<&str>) -> Option<DeleteMode> {
        delete_mode(scope, name, fragment, &self.user_unit_dir)
    }

    /// Forgets a crash-loop sighting, so a unit the owner just stopped or deleted leaves the
    /// failing list straight away.
    fn forget_looping(&self, scope: Scope, name: &str) {
        self.looping.lock().expect("lock poisoned").remove(&(scope, name.to_owned()));
    }

    /// Records a fresh crash-loop sighting and answers whether the unit counts as
    /// crash-looping: seen now, or within [`LOOP_MEMORY`].
    fn crash_loop_seen(&self, scope: Scope, name: &str, now: bool) -> bool {
        let mut looping = self.looping.lock().expect("lock poisoned");
        let key = (scope, name.to_owned());
        if now {
            looping.insert(key, Instant::now());
            return true;
        }
        looping.get(&key).is_some_and(|seen| seen.elapsed() < LOOP_MEMORY)
    }

    /// Units seen crash-looping within [`LOOP_MEMORY`].
    fn recently_looping(&self) -> Vec<(Scope, String)> {
        let mut looping = self.looping.lock().expect("lock poisoned");
        looping.retain(|_, seen| seen.elapsed() < LOOP_MEMORY);
        looping.keys().cloned().collect()
    }

    fn bus(&self, scope: Scope) -> Result<&Connection, SystemdError> {
        match scope {
            Scope::System => self.system.as_ref(),
            Scope::User => self.user.as_ref(),
        }
        .ok_or(SystemdError::Unavailable(scope))
    }

    /// Every service in `scope`, sorted by name. Templates (`foo@.service`) are left out since
    /// they can't run without an instance name.
    pub async fn services(&self, scope: Scope) -> Result<Vec<Service>, SystemdError> {
        let bus = self.bus(scope)?;
        let mut file_states = self.file_states(scope).await?;

        let reply = bus.call_method(Some(DEST), PATH, Some(MANAGER), "ListUnits", &()).await?;
        let units: Vec<UnitRow> = reply.body().deserialize()?;
        let loaded: Vec<_> = units
            .into_iter()
            .filter(|u| u.2 != "not-found")
            .filter_map(|(name, description, _, active, sub, _, path, ..)| {
                Some((UnitName::try_from(name).ok()?, description, active, sub, path))
            })
            .collect();
        // One D-Bus round trip per running service, so ask for all of them at once.
        let extras = futures_util::future::join_all(loaded.iter().map(|(.., active, sub, path)| async move {
            match active.as_str() {
                "active" if sub == "running" => (service_property::<u64>(bus, path, "MemoryCurrent").await, false),
                "activating" | "deactivating" => (None, crash_looping(bus, path).await),
                _ => (None, false),
            }
        }))
        .await;
        let mut services = Vec::new();
        for ((name, description, active, sub, _), (memory, crash_looping)) in loaded.into_iter().zip(extras) {
            let file_state = file_states.remove(name.as_str());
            let memory = memory.filter(|m| *m != UNSET);
            let crash_looping = self.crash_loop_seen(scope, name.as_str(), crash_looping);
            services.push(Service { scope, name, description, active, sub, file_state, memory, crash_looping });
        }
        // Installed but never loaded since boot (disabled and not started).
        for (name, state) in file_states {
            let Ok(unit) = UnitName::try_from(name) else { continue };
            services.push(Service {
                scope,
                name: unit,
                description: String::new(),
                active: "inactive".to_owned(),
                sub: "dead".to_owned(),
                file_state: Some(state),
                memory: None,
                crash_looping: false,
            });
        }
        services.sort_by_key(|s| s.name.as_str().to_lowercase());
        Ok(services)
    }

    /// Installed service unit files and whether they start at boot, cached for [`FILE_STATES_TTL`].
    async fn file_states(&self, scope: Scope) -> Result<HashMap<String, String>, SystemdError> {
        if let Some((read, states)) = self.file_states.lock().expect("lock poisoned").get(&scope)
            && read.elapsed() < FILE_STATES_TTL
        {
            return Ok(states.clone());
        }
        let reply = self.bus(scope)?.call_method(Some(DEST), PATH, Some(MANAGER), "ListUnitFiles", &()).await?;
        let files: Vec<(String, String)> = reply.body().deserialize()?;
        let states: HashMap<String, String> = files
            .into_iter()
            .filter_map(|(path, state)| Some((path.rsplit('/').next()?.to_owned(), state)))
            .filter(|(name, _)| name.ends_with(".service") && !name.ends_with("@.service"))
            .collect();
        self.file_states.lock().expect("lock poisoned").insert(scope, (Instant::now(), states.clone()));
        Ok(states)
    }

    /// Services that failed or are stuck restarting, in both managers. One cheap call each,
    /// so the overview can poll it.
    pub async fn failing(&self) -> Vec<Failing> {
        let mut failing = Vec::new();
        for scope in [Scope::System, Scope::User] {
            let Ok(bus) = self.bus(scope) else { continue };
            // A crash-looping unit cycles through all three between attempts.
            let states = vec!["failed", "activating", "deactivating"];
            let reply = bus.call_method(Some(DEST), PATH, Some(MANAGER), "ListUnitsFiltered", &(states,)).await;
            let Ok(units) = reply.and_then(|r| r.body().deserialize::<Vec<UnitRow>>()) else { continue };
            for (name, _, _, active, _, _, path, ..) in units {
                let Ok(name) = UnitName::try_from(name) else { continue };
                let restarting = active != "failed" && crash_looping(bus, &path).await;
                let restarting = self.crash_loop_seen(scope, name.as_str(), restarting);
                if active == "failed" || restarting {
                    failing.push(Failing { scope, name, restarting });
                }
            }
        }
        // Looping units caught between attempts, momentarily "active".
        for (scope, name) in self.recently_looping() {
            if !failing.iter().any(|f| f.scope == scope && f.name.as_str() == name)
                && let Ok(name) = UnitName::try_from(name)
            {
                failing.push(Failing { scope, name, restarting: true });
            }
        }
        failing.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        failing
    }

    /// One service's state and resource use. Loads the unit if systemd hasn't yet.
    pub async fn details(&self, scope: Scope, name: &UnitName) -> Result<Details, SystemdError> {
        let bus = self.bus(scope)?;
        let reply = bus.call_method(Some(DEST), PATH, Some(MANAGER), "LoadUnit", &(name.as_str(),)).await?;
        let path: OwnedObjectPath = reply.body().deserialize()?;
        let unit = all_properties(bus, &path, "org.freedesktop.systemd1.Unit").await?;
        let service = all_properties(bus, &path, "org.freedesktop.systemd1.Service").await?;
        let text = |props: &HashMap<String, OwnedValue>, key: &str| {
            props.get(key).and_then(|v| String::try_from(v.clone()).ok()).filter(|s| !s.is_empty())
        };
        let number = |props: &HashMap<String, OwnedValue>, key: &str| {
            props.get(key).and_then(|v| u64::try_from(v.clone()).ok()).filter(|n| *n != UNSET && *n != 0)
        };
        let active = text(&unit, "ActiveState").unwrap_or_default();
        Ok(Details {
            service: Service {
                scope,
                name: name.clone(),
                description: text(&unit, "Description").unwrap_or_default(),
                sub: text(&unit, "SubState").unwrap_or_default(),
                file_state: text(&unit, "UnitFileState"),
                memory: if active == "active" { number(&service, "MemoryCurrent") } else { None },
                crash_looping: self.crash_loop_seen(
                    scope,
                    name.as_str(),
                    matches!(active.as_str(), "activating" | "deactivating")
                        && service.get("NRestarts").and_then(|v| u32::try_from(v.clone()).ok()).unwrap_or(0) >= CRASH_LOOP,
                ),
                active,
            },
            main_pid: service.get("MainPID").and_then(|v| u32::try_from(v.clone()).ok()).filter(|p| *p != 0),
            active_since: number(&unit, "ActiveEnterTimestamp").map(|usec| usec / 1_000_000),
            cpu_nsec: number(&service, "CPUUsageNSec"),
            tasks: number(&service, "TasksCurrent"),
            restarts: service.get("NRestarts").and_then(|v| u32::try_from(v.clone()).ok()).filter(|n| *n > 0),
            fragment_path: text(&unit, "FragmentPath"),
        })
    }

    /// Carries out an action and returns what happened, in words for the owner. Start, stop
    /// and restart queue a job; enable, disable and unmask change unit files and reload.
    pub async fn act(&self, scope: Scope, name: &UnitName, action: Action) -> Result<String, SystemdError> {
        let bus = self.bus(scope)?;
        let manager = |method: &'static str| async move {
            bus.call_method(Some(DEST), PATH, Some(MANAGER), method, &(name.as_str(), "replace")).await
        };
        let reload = || async { bus.call_method(Some(DEST), PATH, Some(MANAGER), "Reload", &()).await };
        let message = match action {
            Action::Start => {
                manager("StartUnit").await?;
                format!("Started {name}")
            }
            Action::Stop => {
                manager("StopUnit").await?;
                self.forget_looping(scope, name.as_str());
                format!("Stopped {name}")
            }
            Action::Restart => {
                manager("RestartUnit").await?;
                format!("Restarted {name}")
            }
            Action::Kill => {
                bus.call_method(Some(DEST), PATH, Some(MANAGER), "KillUnit", &(name.as_str(), "all", 9i32)).await?;
                self.forget_looping(scope, name.as_str());
                format!("Sent SIGKILL to every process of {name}")
            }
            Action::Enable => {
                bus.call_method(Some(DEST), PATH, Some(MANAGER), "EnableUnitFiles", &(vec![name.as_str()], false, false))
                    .await?;
                reload().await?;
                format!("{name} will start at boot")
            }
            Action::Disable => {
                bus.call_method(Some(DEST), PATH, Some(MANAGER), "DisableUnitFiles", &(vec![name.as_str()], false)).await?;
                reload().await?;
                format!("{name} won't start at boot")
            }
            Action::Unmask => {
                bus.call_method(Some(DEST), PATH, Some(MANAGER), "UnmaskUnitFiles", &(vec![name.as_str()], false)).await?;
                reload().await?;
                format!("Unmasked {name}; it can be started again")
            }
            Action::Delete => {
                let fragment = self.fragment_path(scope, name).await?;
                let mode = self
                    .delete_mode(scope, name, fragment.as_deref())
                    .ok_or_else(|| SystemdError::Failed(format!("{name} has no unit file to delete")))?;
                manager("StopUnit").await?;
                // Not enabled is fine; a unit may have no [Install] section at all.
                let disable = (vec![name.as_str()], false);
                let _ = bus.call_method(Some(DEST), PATH, Some(MANAGER), "DisableUnitFiles", &disable).await;
                let message = match &mode {
                    DeleteMode::RemoveFile(path) => {
                        std::fs::remove_file(path)
                            .map_err(|e| SystemdError::Failed(format!("Couldn't remove {}: {e}", path.display())))?;
                        format!("Deleted {name} and removed {}", path.display())
                    }
                    DeleteMode::Mask => {
                        bus.call_method(Some(DEST), PATH, Some(MANAGER), "MaskUnitFiles", &(vec![name.as_str()], false, false))
                            .await?;
                        format!("Masked {name}: stopped and blocked from starting until you unmask it")
                    }
                    DeleteMode::StopAndDisable(path) => format!(
                        "Stopped and disabled {name}. Its file belongs to root; to remove it run: sudo rm {} && sudo systemctl daemon-reload",
                        path.display()
                    ),
                };
                reload().await?;
                let _ = bus.call_method(Some(DEST), PATH, Some(MANAGER), "ResetFailedUnit", &(name.as_str(),)).await;
                self.forget_looping(scope, name.as_str());
                message
            }
        };
        if matches!(action, Action::Enable | Action::Disable | Action::Unmask | Action::Delete) {
            self.file_states.lock().expect("lock poisoned").remove(&scope);
        }
        Ok(message)
    }

    /// The unit's file on disk, if it has one.
    async fn fragment_path(&self, scope: Scope, name: &UnitName) -> Result<Option<String>, SystemdError> {
        let bus = self.bus(scope)?;
        let reply = bus.call_method(Some(DEST), PATH, Some(MANAGER), "LoadUnit", &(name.as_str(),)).await?;
        let path: OwnedObjectPath = reply.body().deserialize()?;
        Ok(unit_property::<String>(bus, &path, "org.freedesktop.systemd1.Unit", "FragmentPath").await)
    }
}

/// The unit's journal, newest last. Reading the system journal needs the `wheel`, `adm` or
/// `systemd-journal` group; without it `journalctl` prints only this user's entries.
pub async fn logs(scope: Scope, name: &UnitName, lines: u32) -> Result<String, SystemdError> {
    let unit_flag = match scope {
        Scope::System => format!("--unit={name}"),
        Scope::User => format!("--user-unit={name}"),
    };
    let run = Command::new("journalctl")
        .args([unit_flag.as_str(), "--no-pager", "--output=short-iso", "--quiet"])
        .arg(format!("--lines={lines}"))
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .map_err(|_| SystemdError::Logs("journalctl took too long".to_owned()))?
        .map_err(|e| SystemdError::Logs(e.to_string()))?;
    if !output.status.success() {
        return Err(SystemdError::Logs(String::from_utf8_lossy(&output.stderr).trim().to_owned()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn all_properties(
    bus: &Connection,
    path: &OwnedObjectPath,
    interface: &str,
) -> Result<HashMap<String, OwnedValue>, SystemdError> {
    let reply = bus
        .call_method(Some(DEST), path.as_str(), Some("org.freedesktop.DBus.Properties"), "GetAll", &(interface,))
        .await?;
    Ok(reply.body().deserialize()?)
}

/// One property of a service unit, or `None` if it can't be read.
async fn service_property<T: TryFrom<OwnedValue>>(bus: &Connection, path: &OwnedObjectPath, name: &str) -> Option<T> {
    unit_property(bus, path, "org.freedesktop.systemd1.Service", name).await
}

/// One property of a unit on the given interface, or `None` if it can't be read.
async fn unit_property<T: TryFrom<OwnedValue>>(
    bus: &Connection,
    path: &OwnedObjectPath,
    interface: &str,
    name: &str,
) -> Option<T> {
    let args = (interface, name);
    let reply = bus
        .call_method(Some(DEST), path.as_str(), Some("org.freedesktop.DBus.Properties"), "Get", &args)
        .await
        .ok()?;
    let value: OwnedValue = reply.body().deserialize().ok()?;
    T::try_from(value).ok()
}

/// Whether a starting service has already been restarted [`CRASH_LOOP`] times. A service that
/// can't stay up spends most of its time "activating" between attempts.
async fn crash_looping(bus: &Connection, path: &OwnedObjectPath) -> bool {
    service_property::<u32>(bus, path, "NRestarts").await.unwrap_or(0) >= CRASH_LOOP
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{DeleteMode, Scope, UnitName, delete_mode};

    fn mode(scope: Scope, name: &str, fragment: Option<&str>) -> Option<DeleteMode> {
        let name = UnitName::try_from(name.to_owned()).expect("valid name");
        delete_mode(scope, &name, fragment, Path::new("/home/u/.config/systemd/user"))
    }

    #[test]
    fn delete_picks_the_strongest_safe_step() {
        let own = "/home/u/.config/systemd/user/foo.service";
        assert_eq!(mode(Scope::User, "foo.service", Some(own)), Some(DeleteMode::RemoveFile(own.into())));
        assert_eq!(mode(Scope::User, "pipewire.service", Some("/usr/lib/systemd/user/pipewire.service")), Some(DeleteMode::Mask));
        assert_eq!(mode(Scope::System, "nginx.service", Some("/usr/lib/systemd/system/nginx.service")), Some(DeleteMode::Mask));
        let etc = "/etc/systemd/system/flood.service";
        assert_eq!(mode(Scope::System, "flood.service", Some(etc)), Some(DeleteMode::StopAndDisable(etc.into())));
        // An instance never removes or overwrites its template's file.
        assert_eq!(mode(Scope::User, "foo@a.service", Some("/home/u/.config/systemd/user/foo@.service")), Some(DeleteMode::Mask));
        assert_eq!(mode(Scope::System, "x.service", Some("/run/systemd/generator/x.service")), None);
        assert_eq!(mode(Scope::System, "x.service", None), None);
    }

    #[test]
    fn unit_names_are_services_only() {
        for ok in ["nginx.service", "wg-quick@wg0.service", "user@1000.service", "dirmngr@etc-pacman.d-gnupg.service"] {
            assert!(UnitName::try_from(ok.to_owned()).is_ok(), "{ok}");
        }
        for bad in ["nginx", "nginx.socket", ".service", "--since=x.service", "a b.service", "a/b.service"] {
            assert!(UnitName::try_from(bad.to_owned()).is_err(), "{bad}");
        }
    }
}
