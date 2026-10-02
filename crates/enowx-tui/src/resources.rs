//! What this session costs the machine: the memory and CPU of enx and the
//! processes it started (MCP servers, language servers, the preview browser),
//! and the session's history on disk.
//!
//! The sample is taken OFF the UI thread. Reading the process table and
//! stat-ing the session files is cheap on a quiet machine but not free on a
//! busy Windows box with antivirus in the path, and doing it inline once
//! froze the interface for the length of the read. Here a background task
//! does the work and the UI only reads the latest result.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

/// How often a fresh sample is taken.
const EVERY: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Usage {
    /// enx itself, resident memory in bytes.
    pub own_memory: u64,
    /// The processes enx started, and their children, together.
    pub helpers_memory: u64,
    pub helpers: usize,
    /// CPU of enx and its helpers, percent of one core.
    pub cpu: f32,
    /// The session's transcript and its delegations' on disk, in bytes.
    pub on_disk: u64,
    /// How many delegation transcripts that counts.
    pub branches: usize,
}

pub(crate) struct Sampler {
    /// Filled by the background task; read by the UI each frame.
    latest: Arc<Mutex<Usage>>,
    /// Whether a sample is in flight, so only one runs at a time.
    running: Arc<std::sync::atomic::AtomicBool>,
    next: Instant,
    /// Reused across samples so CPU percentages are measured against the
    /// previous reading, as sysinfo requires.
    system: Arc<Mutex<System>>,
    me: Pid,
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            latest: Arc::new(Mutex::new(Usage::default())),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            next: Instant::now(),
            system: Arc::new(Mutex::new(System::new_with_specifics(
                RefreshKind::nothing(),
            ))),
            me: sysinfo::get_current_pid().unwrap_or(Pid::from_u32(std::process::id())),
        }
    }
}

impl Sampler {
    /// The latest sample. Never blocks on the process table.
    pub(crate) fn usage(&self) -> Usage {
        self.latest.lock().map(|u| *u).unwrap_or_default()
    }

    /// Called each frame. When it is time, start a sample on a blocking task
    /// and return at once; the UI keeps the previous figures until it lands.
    pub(crate) fn tick(&mut self, store: &enowx_core::SessionStore, session: Option<&str>) {
        if Instant::now() < self.next {
            return;
        }
        self.next = Instant::now() + EVERY;
        use std::sync::atomic::Ordering;
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let (latest, running, system, me) = (
            self.latest.clone(),
            self.running.clone(),
            self.system.clone(),
            self.me,
        );
        let store = store.clone();
        let session = session.map(str::to_owned);
        let work = move || {
            let usage = measure(&system, me, &store, session.as_deref());
            if let Ok(mut slot) = latest.lock() {
                *slot = usage;
            }
            running.store(false, Ordering::SeqCst);
        };
        // Off the UI thread when there is a runtime to run it on (always, in
        // the app); inline otherwise, as in a plain test.
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn_blocking(work);
            }
            Err(_) => work(),
        }
    }
}

/// Read memory, CPU and disk once. Runs on a blocking task, never the UI
/// thread.
fn measure(
    system: &Mutex<System>,
    me: Pid,
    store: &enowx_core::SessionStore,
    session: Option<&str>,
) -> Usage {
    let mut usage = Usage::default();
    if let Ok(mut system) = system.lock() {
        // Memory and CPU only, and no disk usage (the expensive part of a
        // full refresh), for every process once.
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_memory().with_cpu(),
        );
        if let Some(process) = system.process(me) {
            usage.own_memory = process.memory();
            usage.cpu = process.cpu_usage();
        }
        // A parent -> children map, built once, so finding enx's descendants
        // is a walk, not a scan of the whole table per node.
        let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
        for (pid, process) in system.processes() {
            if let Some(parent) = process.parent() {
                children.entry(parent).or_default().push(*pid);
            }
        }
        let mut frontier = vec![me];
        let mut seen = HashSet::new();
        while let Some(parent) = frontier.pop() {
            for pid in children.get(&parent).into_iter().flatten() {
                if !seen.insert(*pid) {
                    continue;
                }
                if let Some(process) = system.process(*pid) {
                    usage.helpers += 1;
                    usage.helpers_memory += process.memory();
                    usage.cpu += process.cpu_usage();
                }
                frontier.push(*pid);
            }
        }
    }
    if let Some(id) = session {
        let (bytes, branches) = on_disk(store, id);
        usage.on_disk = bytes;
        usage.branches = branches;
    }
    usage
}

/// The bytes a session and every delegation under it take on disk, and how
/// many delegations that is. Capped so a session with a huge delegation tree
/// cannot turn one sample into thousands of stat calls.
pub(crate) fn on_disk(store: &enowx_core::SessionStore, id: &str) -> (u64, usize) {
    let ids = store.family(id);
    let bytes = ids
        .iter()
        .take(2_000)
        .filter_map(|id| store.file_of(id))
        .filter_map(|path| std::fs::metadata(path).ok())
        .map(|meta| meta.len())
        .sum();
    (bytes, ids.len().saturating_sub(1))
}

/// Bytes as a person reads them: 940 KB, 12.4 MB, 1.2 GB.
pub(crate) fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_a_person_writes_them() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1536), "1.5 KB");
        assert_eq!(human(12_400_000), "11.8 MB");
        assert_eq!(human(250 * 1024 * 1024), "250 MB");
    }

    #[tokio::test]
    async fn this_process_is_measured_off_the_ui_thread() {
        let mut sampler = Sampler::default();
        let store = enowx_core::SessionStore::new(std::env::temp_dir());
        sampler.tick(&store, None);
        // The sample runs on a blocking task; wait for it to land.
        for _ in 0..50 {
            if sampler.usage().own_memory > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(sampler.usage().own_memory > 0, "{:?}", sampler.usage());
    }
}
