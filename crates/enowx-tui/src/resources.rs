//! What this session costs the machine: the memory and CPU of enx and the
//! processes it started (MCP servers, language servers, the preview browser),
//! and the session's history on disk. Sampled every few seconds, not every
//! frame: reading the process table is cheap but not free.

use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

/// How often the figures are read again.
const EVERY: Duration = Duration::from_secs(2);

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
    system: System,
    me: Pid,
    next: Instant,
    pub usage: Usage,
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            system: System::new_with_specifics(RefreshKind::nothing()),
            me: sysinfo::get_current_pid().unwrap_or(Pid::from_u32(std::process::id())),
            next: Instant::now(),
            usage: Usage::default(),
        }
    }
}

impl Sampler {
    /// Read the figures again when it is time. `session` is the open one.
    pub(crate) fn tick(&mut self, store: &enowx_core::SessionStore, session: Option<&str>) {
        if Instant::now() < self.next {
            return;
        }
        self.next = Instant::now() + EVERY;
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_memory().with_cpu(),
        );
        let mut usage = Usage::default();
        if let Some(me) = self.system.process(self.me) {
            usage.own_memory = me.memory();
            usage.cpu = me.cpu_usage();
        }
        // enx's descendants, found by walking DOWN from enx in this one
        // snapshot: start at enx, then its children, then theirs. Walking UP
        // from every process instead trusted each process's recorded parent,
        // and macOS reuses a pid the moment a process dies, so an unrelated
        // process whose parent pid had been recycled to enx's was counted as
        // a helper. Here a process is a helper only if it is actually reached
        // from enx now.
        let mut frontier = vec![self.me];
        let mut seen = std::collections::HashSet::new();
        while let Some(parent) = frontier.pop() {
            for (pid, process) in self.system.processes() {
                if process.parent() != Some(parent) || !seen.insert(*pid) {
                    continue;
                }
                usage.helpers += 1;
                usage.helpers_memory += process.memory();
                usage.cpu += process.cpu_usage();
                frontier.push(*pid);
            }
        }
        if let Some(id) = session {
            let (bytes, branches) = on_disk(store, id);
            usage.on_disk = bytes;
            usage.branches = branches;
        }
        self.usage = usage;
    }
}

/// The bytes a session and every delegation under it take on disk, and how
/// many delegations that is.
pub(crate) fn on_disk(store: &enowx_core::SessionStore, id: &str) -> (u64, usize) {
    let ids = store.family(id);
    let bytes = ids
        .iter()
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

    #[test]
    fn this_process_is_measured() {
        let mut sampler = Sampler::default();
        sampler.tick(&enowx_core::SessionStore::new(std::env::temp_dir()), None);
        assert!(sampler.usage.own_memory > 0, "{:?}", sampler.usage);
    }
}
