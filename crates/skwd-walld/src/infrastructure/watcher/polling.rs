use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const ENTRY_BUDGET_PER_ROOT: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fingerprint {
    len: u64,
    modified_ns: u128,
    changed_ns: i128,
}

impl Fingerprint {
    fn read(path: &Path) -> std::io::Result<(Self, bool, bool)> {
        let metadata = std::fs::symlink_metadata(path)?;
        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |value| value.as_nanos());
        Ok((
            Self { len: metadata.len(), modified_ns, changed_ns: changed_ns(&metadata) },
            metadata.is_dir(),
            metadata.is_file(),
        ))
    }
}

#[cfg(unix)]
fn changed_ns(metadata: &std::fs::Metadata) -> i128 {
    use std::os::unix::fs::MetadataExt;

    i128::from(metadata.ctime()) * 1_000_000_000 + i128::from(metadata.ctime_nsec())
}

#[cfg(not(unix))]
fn changed_ns(_metadata: &std::fs::Metadata) -> i128 {
    0
}

struct Sweep {
    directories: Vec<std::fs::ReadDir>,
    files: BTreeMap<PathBuf, Fingerprint>,
    #[cfg(test)]
    entries_examined: usize,
    #[cfg(test)]
    metadata_reads: usize,
    #[cfg(test)]
    directories_opened: usize,
}

impl Sweep {
    fn start(root: &Path) -> std::io::Result<Self> {
        Ok(Self {
            directories: vec![std::fs::read_dir(root)?],
            files: BTreeMap::new(),
            #[cfg(test)]
            entries_examined: 0,
            #[cfg(test)]
            metadata_reads: 0,
            #[cfg(test)]
            directories_opened: 1,
        })
    }

    fn advance(&mut self, budget: usize) -> Result<bool, String> {
        let mut consumed = 0;
        while consumed < budget {
            let Some(directory) = self.directories.last_mut() else { return Ok(true) };
            let entry = match directory.next() {
                Some(Ok(entry)) => entry,
                Some(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Some(Err(error)) => return Err(error.to_string()),
                None => {
                    self.directories.pop();
                    continue;
                }
            };
            consumed += 1;
            #[cfg(test)]
            {
                self.entries_examined += 1;
            }
            let path = entry.path();
            if skwd_wall_core::paths::is_internal_library_path(&path) {
                continue;
            }
            #[cfg(test)]
            {
                self.metadata_reads += 1;
            }
            let (fingerprint, is_dir, is_file) = match Fingerprint::read(&path) {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("{}: {error}", path.display())),
            };
            if is_dir {
                match std::fs::read_dir(&path) {
                    Ok(directory) => {
                        self.directories.push(directory);
                        #[cfg(test)]
                        {
                            self.directories_opened += 1;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(format!("{}: {error}", path.display())),
                }
            } else if is_file && !super::is_transient(&path) {
                self.files.insert(path, fingerprint);
            }
        }
        Ok(self.directories.is_empty())
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct PollDelta {
    pub(super) changed: Vec<PathBuf>,
    pub(super) removed: Vec<PathBuf>,
    pub(super) initial: bool,
}

pub(super) enum PollAdvance {
    Pending,
    Complete(PollDelta),
    Failed(String),
}

pub(super) struct PollingRoot {
    pub(super) path: PathBuf,
    pub(super) reason: String,
    baseline: BTreeMap<PathBuf, Fingerprint>,
    has_baseline: bool,
    sweep: Option<Sweep>,
    pub(super) last_completed_sweep_unix_ms: Option<u64>,
    pub(super) last_error: Option<String>,
    #[cfg(test)]
    last_sweep_metrics: SweepMetrics,
}

#[cfg(test)]
#[derive(Clone, Copy, Default)]
pub(super) struct SweepMetrics {
    pub(super) entries_examined: usize,
    pub(super) metadata_reads: usize,
    pub(super) directories_opened: usize,
}

impl PollingRoot {
    pub(super) fn new(path: PathBuf, reason: String) -> Self {
        Self {
            path,
            reason,
            baseline: BTreeMap::new(),
            has_baseline: false,
            sweep: None,
            last_completed_sweep_unix_ms: None,
            last_error: None,
            #[cfg(test)]
            last_sweep_metrics: SweepMetrics::default(),
        }
    }

    pub(super) fn advance(&mut self, budget: usize) -> PollAdvance {
        if self.sweep.is_none() {
            match Sweep::start(&self.path) {
                Ok(sweep) => self.sweep = Some(sweep),
                Err(error) => return self.fail(error.to_string()),
            }
        }
        let finished = match self.sweep.as_mut().expect("sweep initialized above").advance(budget) {
            Ok(finished) => finished,
            Err(error) => return self.fail(error),
        };
        if !finished {
            return PollAdvance::Pending;
        }
        let sweep = std::mem::take(&mut self.sweep).expect("finished sweep above");
        #[cfg(test)]
        {
            self.last_sweep_metrics = SweepMetrics {
                entries_examined: sweep.entries_examined,
                metadata_reads: sweep.metadata_reads,
                directories_opened: sweep.directories_opened,
            };
        }
        let next = sweep.files;
        let initial = !self.has_baseline;
        let changed = next
            .iter()
            .filter(|(path, fingerprint)| self.baseline.get(*path) != Some(*fingerprint))
            .map(|(path, _)| path.clone())
            .collect();
        let removed =
            self.baseline.keys().filter(|path| !next.contains_key(*path)).cloned().collect();
        self.baseline = next;
        self.has_baseline = true;
        self.last_error = None;
        self.last_completed_sweep_unix_ms = Some(unix_ms());
        PollAdvance::Complete(PollDelta { changed, removed, initial })
    }

    fn fail(&mut self, error: String) -> PollAdvance {
        self.sweep = None;
        self.last_error = Some(error.clone());
        PollAdvance::Failed(error)
    }

    #[cfg(test)]
    pub(super) fn last_sweep_metrics(&self) -> SweepMetrics {
        self.last_sweep_metrics
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| u64::try_from(value.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
#[path = "polling_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "polling_benchmark.rs"]
mod benchmark;
