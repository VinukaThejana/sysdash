//! AI coding agents: what herdr knows about each one (status, task title, workspace, folder)
//! joined with the sampler's process stats (CPU and memory of the agent and everything it
//! spawned). Agent processes herdr doesn't manage (a `claude` in a plain SSH shell, say) are
//! listed too, with status "running".

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use serde::Deserialize;
use tokio::process::Command;

use crate::metrics::Snapshot;

/// herdr's agent list is reused for this long between requests.
const FRESH: Duration = Duration::from_secs(2);
const HERDR_TIMEOUT: Duration = Duration::from_secs(3);

/// One agent as shown.
#[derive(Debug, Clone)]
pub struct Agent {
    /// `claude`, `codex`, …
    pub kind: String,
    /// herdr's state (`working`, `blocked`, `done`, `idle`, `unknown`), or `running` outside herdr.
    pub status: String,
    /// What it is doing: the terminal title the agent set, else its folder's name.
    pub title: String,
    /// The herdr workspace, or `None` outside herdr.
    pub workspace: Option<String>,
    pub cwd: String,
    pub pid: Option<u32>,
    pub cpu: Option<f32>,
    pub memory: Option<u64>,
    pub run_secs: Option<u64>,
}

impl Agent {
    /// Sorting rank: waiting for the owner first, then working, then the rest.
    pub fn rank(&self) -> u8 {
        match self.status.as_str() {
            "blocked" => 0,
            "working" => 1,
            "running" => 2,
            "done" => 3,
            "idle" => 4,
            _ => 5,
        }
    }
}

/// The agents plus why herdr's half may be missing.
pub struct AgentList {
    pub agents: Vec<Agent>,
    /// Set when herdr is installed but couldn't be asked; process-only agents are still listed.
    /// Without herdr installed this stays `None`: it's optional.
    pub herdr_error: Option<String>,
}

#[derive(Deserialize)]
struct SnapshotReply {
    result: SnapshotResult,
}
#[derive(Deserialize)]
struct SnapshotResult {
    snapshot: HerdrSnapshot,
}
#[derive(Deserialize, Clone, Default)]
struct HerdrSnapshot {
    #[serde(default)]
    agents: Vec<HerdrAgent>,
    #[serde(default)]
    workspaces: Vec<HerdrWorkspace>,
}
#[derive(Deserialize, Clone)]
struct HerdrAgent {
    agent: String,
    #[serde(default)]
    agent_status: String,
    #[serde(default)]
    cwd: String,
    pane_id: String,
    workspace_id: String,
    #[serde(default)]
    terminal_title_stripped: String,
}
#[derive(Deserialize, Clone)]
struct HerdrWorkspace {
    workspace_id: String,
    label: String,
}
#[derive(Deserialize)]
struct ProcessInfoReply {
    result: ProcessInfoResult,
}
#[derive(Deserialize)]
struct ProcessInfoResult {
    process_info: ProcessInfo,
}
#[derive(Deserialize)]
struct ProcessInfo {
    #[serde(default)]
    foreground_processes: Vec<ForegroundProcess>,
}
#[derive(Deserialize)]
struct ForegroundProcess {
    pid: u32,
}

#[derive(Default)]
struct Cache {
    snapshot: Option<(Instant, Result<HerdrSnapshot, String>)>,
    /// herdr pane → the agent's pid there; checked against live agent pids before use.
    pane_pids: HashMap<String, u32>,
}

/// Shared handle; cheap to clone.
#[derive(Clone)]
pub struct Agents {
    herdr: PathBuf,
    /// Also serialises herdr calls from concurrent requests.
    cache: Arc<tokio::sync::Mutex<Cache>>,
}

impl Agents {
    pub fn new() -> Self {
        Self { herdr: herdr_binary(), cache: Default::default() }
    }

    /// Every agent, waiting-for-you first, joined with the stats from `s`.
    pub async fn list(&self, s: &Snapshot) -> AgentList {
        let mut cache = self.cache.lock().await;
        let stale = cache.snapshot.as_ref().is_none_or(|(at, _)| at.elapsed() > FRESH);
        if stale {
            let fresh = self.snapshot().await;
            cache.snapshot = Some((Instant::now(), fresh));
        }
        let (herdr, herdr_error) = match &cache.snapshot {
            Some((_, Ok(snapshot))) => (snapshot.clone(), None),
            // Not installed: show process-found agents without a warning.
            Some((_, Err(_))) if !self.herdr.is_file() => (HerdrSnapshot::default(), None),
            Some((_, Err(e))) => (HerdrSnapshot::default(), Some(e.clone())),
            None => (HerdrSnapshot::default(), None),
        };

        let by_pid: HashMap<u32, &crate::metrics::AgentProcess> = s.agents.iter().map(|a| (a.pid, a)).collect();
        let labels: HashMap<&str, &str> =
            herdr.workspaces.iter().map(|w| (w.workspace_id.as_str(), w.label.as_str())).collect();

        let mut agents = Vec::new();
        let mut claimed = Vec::new();
        for a in &herdr.agents {
            let pid = match cache.pane_pids.get(&a.pane_id).filter(|pid| by_pid.contains_key(pid)) {
                Some(pid) => Some(*pid),
                None => {
                    let found = self.pane_pid(&a.pane_id).await.filter(|pid| by_pid.contains_key(pid));
                    if let Some(pid) = found {
                        cache.pane_pids.insert(a.pane_id.clone(), pid);
                    }
                    found
                }
            };
            let process = pid.and_then(|pid| by_pid.get(&pid));
            claimed.extend(pid);
            agents.push(Agent {
                kind: a.agent.clone(),
                status: if a.agent_status.is_empty() { "unknown".to_owned() } else { a.agent_status.clone() },
                title: title_or_folder(&a.terminal_title_stripped, &a.cwd),
                workspace: labels.get(a.workspace_id.as_str()).map(|l| (*l).to_owned()),
                cwd: a.cwd.clone(),
                pid,
                cpu: process.map(|p| p.cpu),
                memory: process.map(|p| p.memory),
                run_secs: process.map(|p| p.run_secs),
            });
        }
        // Agent processes herdr doesn't manage.
        for p in s.agents.iter().filter(|p| !claimed.contains(&p.pid)) {
            agents.push(Agent {
                kind: p.kind.clone(),
                status: "running".to_owned(),
                title: title_or_folder("", &p.cwd),
                workspace: None,
                cwd: p.cwd.clone(),
                pid: Some(p.pid),
                cpu: Some(p.cpu),
                memory: Some(p.memory),
                run_secs: Some(p.run_secs),
            });
        }
        agents.sort_by(|a, b| a.rank().cmp(&b.rank()).then_with(|| b.cpu.unwrap_or(0.0).total_cmp(&a.cpu.unwrap_or(0.0))));
        AgentList { agents, herdr_error }
    }

    async fn snapshot(&self) -> Result<HerdrSnapshot, String> {
        let out = self.herdr(&["api", "snapshot"]).await?;
        serde_json::from_slice::<SnapshotReply>(&out).map(|r| r.result.snapshot).map_err(|e| format!("herdr answered oddly: {e}"))
    }

    /// The agent process in a herdr pane: its first foreground process.
    async fn pane_pid(&self, pane: &str) -> Option<u32> {
        let out = self.herdr(&["pane", "process-info", "--pane", pane]).await.ok()?;
        let reply: ProcessInfoReply = serde_json::from_slice(&out).ok()?;
        reply.result.process_info.foreground_processes.first().map(|p| p.pid)
    }

    async fn herdr(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        let run = Command::new(&self.herdr).args(args).kill_on_drop(true).output();
        let out = tokio::time::timeout(HERDR_TIMEOUT, run)
            .await
            .map_err(|_| "herdr didn't answer in time".to_owned())?
            .map_err(|e| format!("herdr isn't available: {e}"))?;
        if !out.status.success() {
            return Err(format!("herdr isn't running: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(out.stdout)
    }
}

/// `herdr` on PATH, else where its installer puts it (user services often lack ~/.local/bin).
fn herdr_binary() -> PathBuf {
    if let Some(found) = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|dir| dir.join("herdr"))
        .find(|p| p.is_file())
    {
        return found;
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/bin/herdr")).unwrap_or_else(|| "herdr".into())
}

/// The agent's own title for its task, else the folder it works in.
fn title_or_folder(title: &str, cwd: &str) -> String {
    let title = title.trim();
    if !title.is_empty() {
        return title.to_owned();
    }
    cwd.rsplit('/').find(|s| !s.is_empty()).unwrap_or("agent").to_owned()
}

#[cfg(test)]
mod tests {
    use super::title_or_folder;

    #[test]
    fn titles_fall_back_to_the_folder() {
        assert_eq!(title_or_folder("Fix the login bug", "/home/u/p"), "Fix the login bug");
        assert_eq!(title_or_folder("  ", "/home/u/projects/ting/"), "ting");
        assert_eq!(title_or_folder("", ""), "agent");
    }
}
