//! HTTP routes. Pages and fragments render HTML for htmx; actions return `204` with an
//! `HX-Trigger` header so the page refreshes and shows a toast. Paths are relative in the
//! templates so the dashboard also works behind a reverse proxy under a sub-path.

use std::{path::PathBuf, sync::Arc};

use askama::Template;
use axum::{
    Form, Router,
    extract::{FromRef, Path, Query, Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use tokio::{process::Command, sync::watch};

use crate::{
    agents::Agents,
    metrics::Snapshot,
    storage::{Storage, StorageError, is_clean_absolute},
    systemd::{self, Action, Scope, Systemd, SystemdError, UnitName},
    views::{AgentPane, Band, IndexPage, Overview, StoragePane, ProcessList, ProcessSort, ScopeFilter, ServiceFilter, ServiceList, ServiceSheet},
};

/// Journal lines shown on a service's sheet.
const LOG_LINES: u32 = 200;

/// Everything handlers share; each extracts just the part it needs via `FromRef`.
#[derive(Clone, FromRef)]
pub struct AppState {
    pub snapshot: watch::Receiver<Arc<Snapshot>>,
    pub systemd: Systemd,
    pub storage: Storage,
    pub agents: Agents,
    pub own: OwnUid,
}

/// The uid the server runs as; only its processes can be signalled.
#[derive(Clone, Copy)]
pub struct OwnUid(pub u32);

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/band", get(band))
        .route("/overview", get(overview))
        .route("/processes", get(processes))
        .route("/processes/{pid}/kill", post(kill))
        .route("/services", get(services))
        .route("/services/{scope}/{name}", get(service))
        .route("/services/{scope}/{name}/{action}", post(act))
        .route("/agents", get(agents))
        .route("/storage", get(storage))
        .route("/storage/rescan", post(rescan))
        .route("/storage/delete", post(delete_path))
        .route("/assets/{file}", get(asset))
        .layer(middleware::from_fn(log_actions))
        .with_state(state)
}

/// Logs every action (non-GET) and every failed request. Polls and assets stay quiet.
async fn log_actions(req: Request, next: Next) -> Response {
    let (method, path) = (req.method().clone(), req.uri().path().to_owned());
    let res = next.run(req).await;
    if method != Method::GET || !res.status().is_success() {
        tracing::info!("{method} {path} -> {}", res.status().as_u16());
    }
    res
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Systemd(#[from] SystemdError),
    #[error(transparent)]
    Render(#[from] askama::Error),
    #[error("{0}")]
    Refused(&'static str),
    #[error("{0}")]
    Kill(String),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Systemd(SystemdError::NotAllowed) | Self::Refused(_) => StatusCode::FORBIDDEN,
            Self::Systemd(SystemdError::Unavailable(_)) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Systemd(_) | Self::Kill(_) => StatusCode::BAD_GATEWAY,
            Self::Storage(StorageError::OutsideHome) => StatusCode::FORBIDDEN,
            Self::Storage(StorageError::Missing) => StatusCode::NOT_FOUND,
            Self::Storage(StorageError::Failed(_)) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Render(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        tracing::warn!("{self}");
        (status, self.to_string()).into_response()
    }
}

type Result<T, E = AppError> = std::result::Result<T, E>;

fn render(t: impl Template) -> Result<Html<String>> {
    Ok(Html(t.render()?))
}

/// Empty response that refreshes the page's lists and shows `message` as a toast. With
/// `close_sheet`, the service sheet closes too (its unit is gone).
fn done(message: String, close_sheet: bool) -> impl IntoResponse {
    let mut trigger = serde_json::json!({ "refresh": null, "toast": message });
    if close_sheet {
        trigger["closeSheet"] = serde_json::Value::Null;
    }
    (StatusCode::NO_CONTENT, [("HX-Trigger", trigger.to_string())])
}

#[derive(Deserialize)]
struct IndexQuery {
    /// Columns for the text view (40–200).
    width: Option<u16>,
    /// Present (any value): text without colour codes.
    plain: Option<String>,
    /// Present (any value): the web dashboard's exact pastels instead of the terminal's theme.
    truecolor: Option<String>,
}

/// The dashboard. Clients that don't ask for HTML (`curl`, `wget`, scripts) get the terminal
/// view as text instead: `curl HOST`, `curl 'HOST/?width=72'`, `curl 'HOST/?plain'`.
/// Colours come from the terminal's own palette unless `?truecolor` asks for the pastels.
async fn index(
    State(snapshot): State<watch::Receiver<Arc<Snapshot>>>,
    State(systemd): State<Systemd>,
    State(agents): State<Agents>,
    headers: HeaderMap,
    Query(q): Query<IndexQuery>,
) -> Result<Response> {
    let s = snapshot.borrow().clone();
    let wants_html = headers.get(header::ACCEPT).and_then(|a| a.to_str().ok()).is_some_and(|a| a.contains("text/html"));
    if !wants_html {
        let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("localhost:8091");
        let footer = format!("open http://{host}/ in a browser for processes, services and storage");
        let width = q.width.unwrap_or(100).clamp(40, 200);
        let palette = if q.truecolor.is_some() { &crate::tui::Palette::TRUECOLOR } else { &crate::tui::Palette::ANSI };
        let agents = agents.list(&s).await.agents;
        let text = crate::tui::render_text(&s, &systemd.failing().await, &agents, width, q.plain.is_none(), &footer, palette);
        let headers = [(header::CONTENT_TYPE, "text/plain; charset=utf-8"), (header::VARY, "Accept")];
        return Ok((headers, text).into_response());
    }
    Ok(render(IndexPage {
        host: s.host.name.clone(),
        band: Band::new(&s, systemd.failing().await).render()?,
        overview: Overview::new(&s).render()?,
        processes: ProcessList::new(&s, ProcessSort::Cpu).render()?,
    })?
    .into_response())
}

async fn band(
    State(snapshot): State<watch::Receiver<Arc<Snapshot>>>,
    State(systemd): State<Systemd>,
) -> Result<Html<String>> {
    let s = snapshot.borrow().clone();
    render(Band::new(&s, systemd.failing().await))
}

async fn overview(State(snapshot): State<watch::Receiver<Arc<Snapshot>>>) -> Result<Html<String>> {
    let s = snapshot.borrow().clone();
    render(Overview::new(&s))
}

#[derive(Deserialize)]
struct SortQuery {
    #[serde(default)]
    sort: ProcessSort,
}

async fn processes(
    State(snapshot): State<watch::Receiver<Arc<Snapshot>>>,
    Query(q): Query<SortQuery>,
) -> Result<Html<String>> {
    let s = snapshot.borrow().clone();
    render(ProcessList::new(&s, q.sort))
}

/// Sends SIGTERM to one of the server user's own processes.
async fn kill(State(own): State<OwnUid>, Path(pid): Path<u32>) -> Result<impl IntoResponse> {
    use std::os::unix::fs::MetadataExt;
    let owner = std::fs::metadata(format!("/proc/{pid}")).map_err(|_| AppError::Refused("That process has already exited"))?.uid();
    if owner != own.0 || pid <= 1 {
        return Err(AppError::Refused("Only your own processes can be stopped here"));
    }
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default().trim().to_owned();
    let output = Command::new("kill")
        .args(["-s", "TERM", &pid.to_string()])
        .output()
        .await
        .map_err(|e| AppError::Kill(e.to_string()))?;
    if !output.status.success() {
        return Err(AppError::Kill(String::from_utf8_lossy(&output.stderr).trim().to_owned()));
    }
    Ok(done(format!("Sent stop signal to {name} ({pid})"), false))
}

async fn services(State(systemd): State<Systemd>, Query(filter): Query<ServiceFilter>) -> Result<Html<String>> {
    let scopes: &[Scope] = match filter.scope {
        ScopeFilter::All => &[Scope::System, Scope::User],
        ScopeFilter::System => &[Scope::System],
        ScopeFilter::User => &[Scope::User],
    };
    let (mut all, mut errors) = (Vec::new(), Vec::new());
    for result in futures_util::future::join_all(scopes.iter().map(|&scope| systemd.services(scope))).await {
        match result {
            Ok(services) => all.extend(services),
            Err(e) => errors.push(e.to_string()),
        }
    }
    render(ServiceList::new(all, &filter, errors))
}

async fn service(
    State(systemd): State<Systemd>,
    Path((scope, name)): Path<(Scope, UnitName)>,
) -> Result<Html<String>> {
    let (details, logs) = tokio::join!(systemd.details(scope, &name), systemd::logs(scope, &name, LOG_LINES));
    let details = details?;
    let delete = systemd.delete_mode(scope, &name, details.fragment_path.as_deref());
    render(ServiceSheet::new(details, logs.map_err(|e| e.to_string()), delete))
}

async fn act(
    State(systemd): State<Systemd>,
    Path((scope, name, action)): Path<(Scope, UnitName, Action)>,
) -> Result<impl IntoResponse> {
    let message = systemd.act(scope, &name, action).await?;
    Ok(done(message, action == Action::Delete))
}

/// The agents panel: herdr's agents and stray agent processes, with their load.
async fn agents(
    State(snapshot): State<watch::Receiver<Arc<Snapshot>>>,
    State(agents): State<Agents>,
    State(storage): State<Storage>,
) -> Result<Html<String>> {
    let s = snapshot.borrow().clone();
    render(AgentPane::new(agents.list(&s).await, storage.home()))
}

#[derive(Deserialize)]
struct StorageQuery {
    path: Option<String>,
}

/// One folder of the disk-usage browser; anything not a clean absolute path shows `/`.
async fn storage(State(storage): State<Storage>, Query(q): Query<StorageQuery>) -> Result<Html<String>> {
    let path = q.path.map(PathBuf::from).filter(|p| is_clean_absolute(p)).unwrap_or_else(|| PathBuf::from("/"));
    render(StoragePane::new(storage.list(&path), &storage))
}

async fn rescan(State(storage): State<Storage>) -> impl IntoResponse {
    storage.rescan();
    done("Rescanning the disk".to_owned(), false)
}

#[derive(Deserialize)]
struct DeleteForm {
    path: String,
}

/// Permanently deletes a file or folder inside home.
async fn delete_path(State(storage): State<Storage>, Form(form): Form<DeleteForm>) -> Result<impl IntoResponse> {
    let deleted = storage.delete(PathBuf::from(form.path)).await?;
    let shown = deleted.path.strip_prefix(storage.home()).map_or_else(
        |_| deleted.path.display().to_string(),
        |rest| format!("~/{}", rest.display()),
    );
    let files = if deleted.files <= 1 { String::new() } else { format!(" in {} files", deleted.files) };
    Ok(done(format!("Deleted {shown}: freed {}{files}", crate::views::bytes(deleted.bytes)), false))
}

async fn asset(Path(file): Path<String>) -> Response {
    if file == "archivo.woff2" {
        // The font never changes, so the browser may keep it.
        let body: &'static [u8] = include_bytes!("../assets/fonts/Archivo.woff2");
        let headers = [(header::CONTENT_TYPE, "font/woff2"), (header::CACHE_CONTROL, "public, max-age=31536000, immutable")];
        return (headers, body).into_response();
    }
    let (body, content_type): (&'static [u8], _) = match file.as_str() {
        "app.css" => (include_bytes!("../assets/app.css"), "text/css"),
        "app.js" => (include_bytes!("../assets/app.js"), "text/javascript"),
        "htmx.min.js" => (include_bytes!("../assets/htmx.min.js"), "text/javascript"),
        "idiomorph-ext.min.js" => (include_bytes!("../assets/idiomorph-ext.min.js"), "text/javascript"),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    ([(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}
