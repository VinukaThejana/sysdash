//! Disk usage. A background scan walks the whole filesystem once and keeps an index of every
//! directory's size; browsing reads the index (folders) plus a live listing (files), so it is
//! instant after the first scan. Sizes are disk usage like `du` (allocated blocks), with hard
//! links counted once. Virtual filesystems (/proc, /sys, tmpfs, …) are skipped.
//!
//! Anything readable can be browsed; only paths inside the owner's home can be deleted.

use std::{
    collections::{HashSet, VecDeque},
    fs,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

/// The scan's root.
const ROOT: &str = "/";
/// Mount points never worth walking, whatever /proc/self/mounts says.
const ALWAYS_SKIP: [&str; 4] = ["/proc", "/sys", "/dev", "/run"];
/// Filesystem types that hold no disk data.
const PSEUDO_FS: [&str; 22] = [
    "proc", "sysfs", "devtmpfs", "devpts", "tmpfs", "cgroup", "cgroup2", "mqueue", "debugfs", "tracefs",
    "securityfs", "pstore", "bpf", "configfs", "fusectl", "hugetlbfs", "efivarfs", "autofs", "binfmt_misc",
    "nsfs", "overlay", "ramfs",
];
/// A ready index nobody has looked at for this long is dropped to free its memory.
const IDLE_DROP: Duration = Duration::from_secs(30 * 60);
/// Entries listed per folder; the rest are summed into one line.
const LISTED: usize = 100;

/// One directory in the index. Files are not stored, only counted into their directory.
struct Node {
    name: Box<str>,
    parent: u32,
    /// Bytes on disk, the directory itself and everything below it (after accumulation).
    size: u64,
    /// Files below it.
    files: u64,
    children: Vec<u32>,
    unreadable: bool,
}

/// The scan result.
struct Index {
    nodes: Vec<Node>,
    finished: SystemTime,
    took: Duration,
    unreadable: u64,
}

/// Live counters while a scan runs.
#[derive(Default)]
struct Progress {
    dirs: AtomicU64,
    bytes: AtomicU64,
}

enum State {
    Idle,
    Scanning(Arc<Progress>),
    Ready { index: Index, used: Instant },
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Only files inside your home folder can be deleted here")]
    OutsideHome,
    #[error("That path doesn't exist any more")]
    Missing,
    #[error("{0}")]
    Failed(String),
}

/// What a browse request gets back.
pub enum Listing {
    /// A scan is running (or just started).
    Scanning { dirs: u64, bytes: u64 },
    Folder(Folder),
    /// The path is not in the index (created after the scan, or unreadable).
    NotIndexed { path: PathBuf },
}

pub struct Folder {
    pub path: PathBuf,
    pub size: u64,
    pub files: u64,
    /// Children by size, largest first, at most [`LISTED`].
    pub entries: Vec<Entry>,
    /// The entries left out, and their combined size.
    pub more: usize,
    pub more_size: u64,
    pub unreadable: bool,
    pub scanned: SystemTime,
    pub took: Duration,
    pub unreadable_dirs: u64,
}

pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    /// Files inside, for folders.
    pub files: Option<u64>,
    pub unreadable: bool,
}

/// What a delete freed.
pub struct Deleted {
    pub path: PathBuf,
    pub bytes: u64,
    pub files: u64,
}

/// Shared handle; cheap to clone.
#[derive(Clone)]
pub struct Storage {
    state: Arc<Mutex<State>>,
    home: PathBuf,
}

impl Storage {
    pub fn new(home: PathBuf) -> Self {
        let storage = Self { state: Arc::new(Mutex::new(State::Idle)), home };
        tokio::spawn(storage.clone().drop_when_idle());
        storage
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Whether `path` may be deleted: strictly inside home, written without `..` tricks.
    pub fn deletable(&self, path: &Path) -> bool {
        path.starts_with(&self.home) && path != self.home && is_clean_absolute(path)
    }

    /// Starts a scan unless one is running. A ready index stays usable until it finishes.
    pub fn rescan(&self) {
        let progress = Arc::new(Progress::default());
        {
            let mut state = self.state.lock().expect("lock poisoned");
            if matches!(*state, State::Scanning(_)) {
                return;
            }
            *state = State::Scanning(progress.clone());
        }
        let state = self.state.clone();
        tokio::task::spawn_blocking(move || {
            let index = scan(Path::new(ROOT), &skipped_mounts(), &progress);
            tracing::info!("storage scan: {} folders in {:?}", index.nodes.len(), index.took);
            *state.lock().expect("lock poisoned") = State::Ready { index, used: Instant::now() };
        });
    }

    /// The folder at `path`, starting the first scan if there is no index yet.
    pub fn list(&self, path: &Path) -> Listing {
        let mut state = self.state.lock().expect("lock poisoned");
        match &mut *state {
            State::Idle => {
                drop(state);
                self.rescan();
                Listing::Scanning { dirs: 0, bytes: 0 }
            }
            State::Scanning(p) => {
                Listing::Scanning { dirs: p.dirs.load(Ordering::Relaxed), bytes: p.bytes.load(Ordering::Relaxed) }
            }
            State::Ready { index, used } => {
                *used = Instant::now();
                match index.find(path) {
                    Some(i) => Listing::Folder(index.folder(i, path)),
                    None => Listing::NotIndexed { path: path.to_owned() },
                }
            }
        }
    }

    /// Size and file count of a path, for the confirmation: from the index for folders,
    /// from the file itself otherwise.
    pub fn measure(&self, path: &Path) -> Option<(u64, u64)> {
        if let State::Ready { index, .. } = &*self.state.lock().expect("lock poisoned")
            && let Some(i) = index.find(path)
        {
            let node = &index.nodes[i as usize];
            return Some((node.size, node.files));
        }
        let meta = fs::symlink_metadata(path).ok()?;
        (!meta.is_dir()).then(|| (meta.blocks() * 512, 1))
    }

    /// Deletes a file, symlink or folder inside home, permanently, and updates the index.
    pub async fn delete(&self, path: PathBuf) -> Result<Deleted, StorageError> {
        if !self.deletable(&path) {
            return Err(StorageError::OutsideHome);
        }
        // A symlinked folder on the way could lead outside home; resolve the parent to be sure.
        let parent = path.parent().ok_or(StorageError::OutsideHome)?;
        let real_parent = fs::canonicalize(parent).map_err(|_| StorageError::Missing)?;
        let real_home = fs::canonicalize(&self.home).map_err(|e| StorageError::Failed(e.to_string()))?;
        if !real_parent.starts_with(&real_home) {
            return Err(StorageError::OutsideHome);
        }
        let meta = fs::symlink_metadata(&path).map_err(|_| StorageError::Missing)?;
        let (bytes, files) = self.measure(&path).unwrap_or((meta.blocks() * 512, 1));
        let (target, is_dir) = (path.clone(), meta.is_dir());
        tokio::task::spawn_blocking(move || {
            // remove_dir_all never follows symlinks inside the tree.
            if is_dir { fs::remove_dir_all(&target) } else { fs::remove_file(&target) }
        })
        .await
        .map_err(|e| StorageError::Failed(e.to_string()))?
        .map_err(|e| StorageError::Failed(format!("Couldn't delete {}: {e}", path.display())))?;

        if let State::Ready { index, .. } = &mut *self.state.lock().expect("lock poisoned") {
            index.forget(&path, is_dir, bytes, files);
        }
        Ok(Deleted { path, bytes, files })
    }

    /// Runs forever: frees an index nobody has browsed for [`IDLE_DROP`].
    async fn drop_when_idle(self) {
        loop {
            tokio::time::sleep(Duration::from_secs(300)).await;
            let mut state = self.state.lock().expect("lock poisoned");
            if matches!(&*state, State::Ready { used, .. } if used.elapsed() > IDLE_DROP) {
                *state = State::Idle;
                tracing::info!("storage index dropped after {IDLE_DROP:?} idle");
            }
        }
    }
}

impl Index {
    /// The node for an absolute path, walking down from the root.
    fn find(&self, path: &Path) -> Option<u32> {
        if !is_clean_absolute(path) {
            return None;
        }
        let mut at = 0u32;
        for part in path.components().skip(1) {
            let name = part.as_os_str().to_str()?;
            at = *self.nodes[at as usize].children.iter().find(|&&c| &*self.nodes[c as usize].name == name)?;
        }
        Some(at)
    }

    fn folder(&self, at: u32, path: &Path) -> Folder {
        let node = &self.nodes[at as usize];
        let mut entries: Vec<Entry> = node
            .children
            .iter()
            .map(|&c| {
                let child = &self.nodes[c as usize];
                Entry {
                    name: child.name.to_string(),
                    path: path.join(&*child.name),
                    is_dir: true,
                    size: child.size,
                    files: Some(child.files),
                    unreadable: child.unreadable,
                }
            })
            .collect();
        // Files come from a live listing; folders made since the scan are not shown.
        if let Ok(dir) = fs::read_dir(path) {
            for entry in dir.flatten() {
                let Ok(meta) = entry.metadata() else { continue };
                if meta.is_dir() {
                    continue;
                }
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
                entries.push(Entry {
                    path: path.join(&name),
                    name,
                    is_dir: false,
                    size: meta.blocks() * 512,
                    files: None,
                    unreadable: false,
                });
            }
        }
        entries.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
        let rest = entries.split_off(entries.len().min(LISTED));
        Folder {
            path: path.to_owned(),
            size: node.size,
            files: node.files,
            entries,
            more: rest.len(),
            more_size: rest.iter().map(|e| e.size).sum(),
            unreadable: node.unreadable,
            scanned: self.finished,
            took: self.took,
            unreadable_dirs: self.unreadable,
        }
    }

    /// Takes a deleted path out of the totals of every folder above it.
    fn forget(&mut self, path: &Path, is_dir: bool, bytes: u64, files: u64) {
        let mut at = if is_dir {
            let Some(gone) = self.find(path) else { return };
            let parent = self.nodes[gone as usize].parent;
            self.nodes[parent as usize].children.retain(|&c| c != gone);
            parent
        } else {
            let Some(parent) = path.parent().and_then(|p| self.find(p)) else { return };
            parent
        };
        loop {
            let node = &mut self.nodes[at as usize];
            node.size = node.size.saturating_sub(bytes);
            node.files = node.files.saturating_sub(files);
            if at == 0 {
                break;
            }
            at = node.parent;
        }
    }
}

/// Walks `root` breadth first without following symlinks, skipping `skip` mount points.
fn scan(root: &Path, skip: &HashSet<PathBuf>, progress: &Progress) -> Index {
    let started = Instant::now();
    let mut nodes = vec![Node { name: "".into(), parent: 0, size: 0, files: 0, children: Vec::new(), unreadable: false }];
    let mut queue = VecDeque::from([(0u32, root.to_owned())]);
    let mut linked = HashSet::new();
    let mut unreadable = 0;
    while let Some((at, dir)) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&dir) else {
            nodes[at as usize].unreadable = true;
            unreadable += 1;
            continue;
        };
        for entry in entries.flatten() {
            // DirEntry::metadata does not follow symlinks.
            let Ok(meta) = entry.metadata() else { continue };
            let bytes = meta.blocks() * 512;
            progress.bytes.fetch_add(bytes, Ordering::Relaxed);
            if meta.is_dir() {
                let path = entry.path();
                let Some(name) = entry.file_name().to_str().map(Box::<str>::from) else { continue };
                if skip.contains(&path) {
                    continue;
                }
                let child = u32::try_from(nodes.len()).expect("fewer than 4 billion folders");
                nodes.push(Node { name, parent: at, size: bytes, files: 0, children: Vec::new(), unreadable: false });
                nodes[at as usize].children.push(child);
                queue.push_back((child, path));
                progress.dirs.fetch_add(1, Ordering::Relaxed);
            } else {
                if meta.nlink() > 1 && !linked.insert((meta.dev(), meta.ino())) {
                    continue;
                }
                let node = &mut nodes[at as usize];
                node.size += bytes;
                node.files += 1;
            }
        }
    }
    // Children always come after their parent, so one backwards pass sums every subtree.
    for i in (1..nodes.len()).rev() {
        let (parent, size, files) = (nodes[i].parent as usize, nodes[i].size, nodes[i].files);
        nodes[parent].size += size;
        nodes[parent].files += files;
    }
    Index { nodes, finished: SystemTime::now(), took: started.elapsed(), unreadable }
}

/// Mount points of virtual filesystems, from /proc/self/mounts.
fn skipped_mounts() -> HashSet<PathBuf> {
    let mut skip: HashSet<PathBuf> = ALWAYS_SKIP.iter().map(PathBuf::from).collect();
    if let Ok(mounts) = fs::read_to_string("/proc/self/mounts") {
        for line in mounts.lines() {
            let mut fields = line.split(' ');
            let (Some(_), Some(point), Some(kind)) = (fields.next(), fields.next(), fields.next()) else { continue };
            if PSEUDO_FS.contains(&kind) || kind.starts_with("fuse.") {
                skip.insert(PathBuf::from(unescape_mount(point)));
            }
        }
    }
    skip.remove(Path::new(ROOT));
    skip
}

/// /proc/self/mounts writes spaces and friends as octal escapes (`\040`).
fn unescape_mount(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() && bytes[i + 1..i + 4].iter().all(|b| (b'0'..=b'7').contains(b)) {
            let code = (bytes[i + 1] - b'0') * 64 + (bytes[i + 2] - b'0') * 8 + (bytes[i + 3] - b'0');
            out.push(code);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Absolute, and made only of normal components: no `.`, `..` or repeated roots.
pub fn is_clean_absolute(path: &Path) -> bool {
    let mut parts = path.components();
    matches!(parts.next(), Some(Component::RootDir)) && parts.all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{is_clean_absolute, unescape_mount};

    #[test]
    fn only_clean_absolute_paths_pass() {
        assert!(is_clean_absolute(Path::new("/home/u/projects")));
        assert!(is_clean_absolute(Path::new("/")));
        assert!(!is_clean_absolute(Path::new("/home/u/../../etc")));
        assert!(!is_clean_absolute(Path::new("home/u")));
        // `.` in the middle is dropped by Path::components, so it resolves to /home/u: harmless.
        assert!(is_clean_absolute(Path::new("/home/./u")));
    }

    #[test]
    fn mount_points_unescape() {
        assert_eq!(unescape_mount(r"/mnt/My\040Drive"), "/mnt/My Drive");
        assert_eq!(unescape_mount("/plain"), "/plain");
    }
}
