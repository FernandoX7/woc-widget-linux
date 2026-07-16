//! Frozen-format history persistence and coalesced background writes.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;

use super::normalizer::normalize_samples;
use super::HistorySample;

pub const MAX_HISTORY_BYTES: u64 = 16 * 1024 * 1024;
pub const SAMPLES_PER_FLUSH: u64 = 12;
pub const MAX_FLUSH_INTERVAL: Duration = Duration::from_secs(120);

#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Decode(serde_json::Error),
    TooLarge { bytes: u64 },
    WorkerStopped,
    DrainTimedOut,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "history I/O failed: {error}"),
            Self::Decode(error) => write!(f, "history JSON is invalid: {error}"),
            Self::TooLarge { bytes } => write!(
                f,
                "history file is {bytes} bytes (limit is {MAX_HISTORY_BYTES})"
            ),
            Self::WorkerStopped => f.write_str("history persistence worker stopped"),
            Self::DrainTimedOut => f.write_str("history final drain timed out"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(value: serde_json::Error) -> Self {
        Self::Decode(value)
    }
}

#[derive(Clone, Debug)]
pub struct FileHistoryStore {
    path: PathBuf,
}

impl FileHistoryStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The Linux history path, honoring `XDG_DATA_HOME` exactly when it is set.
    pub fn xdg() -> io::Result<Self> {
        Ok(Self::new(history_path()?))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn backup_path(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_owned();
        name.push(".backup");
        PathBuf::from(name)
    }

    pub fn load(&self) -> Result<Vec<HistorySample>, StoreError> {
        let primary = read_and_decode(&self.path);
        match primary {
            FileState::Valid(raw, samples) => self.finish_load(raw, samples, false),
            FileState::Missing => match self.load_backup(false) {
                Ok(samples) => Ok(samples),
                Err(BackupLoadError::Missing) => Ok(Vec::new()),
                Err(BackupLoadError::Invalid(error) | BackupLoadError::Store(error)) => Err(error),
            },
            FileState::Invalid(primary_error) => match self.load_backup(true) {
                Ok(samples) => Ok(samples),
                Err(BackupLoadError::Missing) => Err(primary_error),
                Err(BackupLoadError::Invalid(_)) => Err(primary_error),
                Err(BackupLoadError::Store(error)) => Err(error),
            },
        }
    }

    fn load_backup(&self, primary_existed: bool) -> Result<Vec<HistorySample>, BackupLoadError> {
        match read_and_decode(&self.backup_path()) {
            FileState::Missing => {
                if primary_existed {
                    Err(BackupLoadError::Missing)
                } else {
                    Ok(Vec::new())
                }
            }
            FileState::Invalid(error) => Err(BackupLoadError::Invalid(error)),
            FileState::Valid(raw, samples) => self
                .finish_load(raw, samples, true)
                .map_err(BackupLoadError::Store),
        }
    }

    fn finish_load(
        &self,
        raw: Vec<u8>,
        samples: Vec<HistorySample>,
        restore_primary: bool,
    ) -> Result<Vec<HistorySample>, StoreError> {
        let normalized = normalize_samples(&samples, Utc::now());
        let repaired = normalized != samples;
        if repaired {
            // Repair persistence follows the same validation/rotation rules as every save.
            self.save(&normalized)?;
        } else if restore_primary {
            // Restore the known-good sidecar without rotating the corrupt primary.
            let _ = atomic_write(&self.path, &raw);
        }
        Ok(normalized)
    }

    pub fn save(&self, samples: &[HistorySample]) -> Result<(), StoreError> {
        let normalized = normalize_samples(samples, Utc::now());
        if normalized.is_empty() {
            let primary_result = remove_if_present(&self.path);
            let backup_result = remove_if_present(&self.backup_path());
            return match (primary_result, backup_result) {
                (Err(error), _) | (Ok(()), Err(error)) => Err(StoreError::Io(error)),
                (Ok(()), Ok(())) => Ok(()),
            };
        }

        let encoded = serde_json::to_vec(&normalized)?;
        check_size(encoded.len() as u64)?;
        if let FileState::Valid(raw, _) = read_and_decode(&self.path) {
            atomic_write(&self.backup_path(), &raw)?;
        }
        atomic_write(&self.path, &encoded)
    }
}

pub fn history_path() -> io::Result<PathBuf> {
    let base = match std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => {
            let home = std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "neither XDG_DATA_HOME nor HOME is set",
                    )
                })?;
            PathBuf::from(home).join(".local/share")
        }
    };
    Ok(base
        .join(crate::config::history::DIRECTORY_NAME)
        .join(crate::config::history::FILE_NAME))
}

enum FileState {
    Missing,
    Valid(Vec<u8>, Vec<HistorySample>),
    Invalid(StoreError),
}

enum BackupLoadError {
    Missing,
    Invalid(StoreError),
    Store(StoreError),
}

fn read_and_decode(path: &Path) -> FileState {
    match read_capped(path) {
        Ok(Some(raw)) => match serde_json::from_slice(&raw) {
            Ok(samples) => FileState::Valid(raw, samples),
            Err(error) => FileState::Invalid(StoreError::Decode(error)),
        },
        Ok(None) => FileState::Missing,
        Err(error) => FileState::Invalid(error),
    }
}

fn read_capped(path: &Path) -> Result<Option<Vec<u8>>, StoreError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "history snapshot is not a regular file",
        )
        .into());
    }
    check_size(metadata.len())?;

    let mut file = File::open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)?;
    check_size(bytes.len() as u64)?;
    Ok(Some(bytes))
}

fn check_size(bytes: u64) -> Result<(), StoreError> {
    if bytes > MAX_HISTORY_BYTES {
        Err(StoreError::TooLarge { bytes })
    } else {
        Ok(())
    }
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "history path has no parent"))?;
    fs::create_dir_all(parent)?;
    let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), id));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok::<_, io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(StoreError::Io)
}

/// A serial background writer. Every command is generation-tagged by the worker, so a
/// successful write only acknowledges the exact snapshot it persisted.
pub struct HistoryWriteCoalescer {
    tx: Sender<Command>,
}

enum Command {
    Record(HistorySample),
    Snapshot(Sender<Vec<HistorySample>>),
    Clear(Sender<Result<(), StoreError>>),
    PersistenceError(Sender<Option<String>>),
    Flush(Sender<Result<(), StoreError>>),
    FinalDrain(Sender<Result<(), StoreError>>),
}

impl HistoryWriteCoalescer {
    pub fn new(store: FileHistoryStore, initial: Vec<HistorySample>) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("wockit-history".into())
            .spawn(move || run_worker(store, initial, rx))
            .expect("failed to spawn history persistence worker");
        Self { tx }
    }

    pub fn record(&self, sample: HistorySample) -> Result<(), StoreError> {
        self.tx
            .send(Command::Record(sample))
            .map_err(|_| StoreError::WorkerStopped)
    }

    pub fn flush(&self) -> Result<(), StoreError> {
        self.request(Command::Flush)
    }

    /// Returns the worker's current in-memory history, including records not yet flushed.
    pub fn snapshot(&self) -> Result<Vec<HistorySample>, StoreError> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Command::Snapshot(tx))
            .map_err(|_| StoreError::WorkerStopped)?;
        rx.recv().map_err(|_| StoreError::WorkerStopped)
    }

    /// Removes the primary and backup snapshots and only then clears the worker's memory.
    /// A failed delete leaves the in-memory samples intact so the UI can surface the error
    /// without silently presenting a successful clear.
    pub fn clear(&self) -> Result<(), StoreError> {
        self.request(Command::Clear)
    }

    /// Last persistence failure, cleared by the next successful disk write.
    pub fn persistence_error(&self) -> Result<Option<String>, StoreError> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Command::PersistenceError(tx))
            .map_err(|_| StoreError::WorkerStopped)?;
        rx.recv().map_err(|_| StoreError::WorkerStopped)
    }

    /// Makes one bounded attempt at the latest failed generation; it never retry-loops.
    pub fn final_drain(&self, timeout: Duration) -> Result<(), StoreError> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Command::FinalDrain(tx))
            .map_err(|_| StoreError::WorkerStopped)?;
        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(StoreError::DrainTimedOut),
            Err(RecvTimeoutError::Disconnected) => Err(StoreError::WorkerStopped),
        }
    }

    fn request(
        &self,
        make: fn(Sender<Result<(), StoreError>>) -> Command,
    ) -> Result<(), StoreError> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(make(tx))
            .map_err(|_| StoreError::WorkerStopped)?;
        rx.recv().map_err(|_| StoreError::WorkerStopped)?
    }
}

fn run_worker(store: FileHistoryStore, initial: Vec<HistorySample>, rx: Receiver<Command>) {
    let mut samples = initial;
    let mut generation = 0_u64;
    let mut persisted_generation = 0_u64;
    let mut last_success = Instant::now();
    let mut persistence_error = None;
    while let Ok(command) = rx.recv() {
        match command {
            Command::Record(sample) => {
                samples.push(sample);
                generation += 1;
                let should_flush = generation.saturating_sub(persisted_generation)
                    >= SAMPLES_PER_FLUSH
                    || last_success.elapsed() >= MAX_FLUSH_INTERVAL;
                if should_flush {
                    match store.save(&samples) {
                        Ok(()) => {
                            persisted_generation = generation;
                            last_success = Instant::now();
                            persistence_error = None;
                        }
                        Err(error) => persistence_error = Some(error.to_string()),
                    }
                }
            }
            Command::Snapshot(reply) => {
                let _ = reply.send(samples.clone());
            }
            Command::Clear(reply) => {
                let result = store.save(&[]);
                if result.is_ok() {
                    samples.clear();
                    generation = generation.saturating_add(1);
                    persisted_generation = generation;
                    last_success = Instant::now();
                    persistence_error = None;
                } else if let Err(error) = &result {
                    persistence_error = Some(error.to_string());
                }
                let _ = reply.send(result);
            }
            Command::PersistenceError(reply) => {
                let _ = reply.send(persistence_error.clone());
            }
            Command::Flush(reply) | Command::FinalDrain(reply) => {
                let snapshot_generation = generation;
                let result = if snapshot_generation == persisted_generation {
                    Ok(())
                } else {
                    store.save(&samples)
                };
                if result.is_ok() {
                    persisted_generation = snapshot_generation;
                    last_success = Instant::now();
                    persistence_error = None;
                } else if let Err(error) = &result {
                    persistence_error = Some(error.to_string());
                }
                let _ = reply.send(result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use chrono::Timelike;
    use std::sync::Arc;

    fn dir(name: &str) -> PathBuf {
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("wockit-{name}-{}-{id}", std::process::id()))
    }

    fn sample(seconds: i64, count: i64) -> HistorySample {
        HistorySample {
            date: Utc::now().with_nanosecond(0).unwrap() + ChronoDuration::seconds(seconds),
            count,
        }
    }

    #[test]
    fn round_trip_and_mac_era_shape() {
        let root = dir("roundtrip");
        let store = FileHistoryStore::new(root.join("history.json"));
        let date = Utc::now().with_nanosecond(123_456_789).unwrap() - ChronoDuration::hours(1);
        let samples = vec![HistorySample { date, count: 187 }];
        store.save(&samples).unwrap();
        let expected_date = date.with_nanosecond(0).unwrap();
        assert_eq!(
            store.load().unwrap(),
            vec![HistorySample {
                date: expected_date,
                count: 187,
            }]
        );
        let encoded = fs::read_to_string(store.path()).unwrap();
        assert_eq!(
            encoded,
            format!(
                r#"[{{"date":"{}","count":187}}]"#,
                expected_date.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            )
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_primary_recovers_and_restores_from_backup() {
        let root = dir("recover");
        let store = FileHistoryStore::new(root.join("history.json"));
        fs::create_dir_all(&root).unwrap();
        let wanted = sample(0, 42);
        fs::write(store.path(), b"not json").unwrap();
        fs::write(
            store.backup_path(),
            serde_json::to_vec(&vec![wanted.clone()]).unwrap(),
        )
        .unwrap();
        assert_eq!(store.load().unwrap(), vec![wanted.clone()]);
        assert_eq!(
            serde_json::from_slice::<Vec<HistorySample>>(&fs::read(store.path()).unwrap()).unwrap(),
            vec![wanted]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn torn_primary_recovers_exact_backup_bytes_and_ignores_orphan_temp() {
        let root = dir("torn");
        let store = FileHistoryStore::new(root.join("history.json"));
        fs::create_dir_all(&root).unwrap();
        let wanted = sample(0, 42);
        let backup_bytes = serde_json::to_vec(&vec![wanted.clone()]).unwrap();
        fs::write(store.path(), br#"[{"date":"2026-07-14T"#).unwrap();
        fs::write(store.backup_path(), &backup_bytes).unwrap();
        fs::write(
            root.join(".history.json.interrupted.tmp"),
            b"partial replacement",
        )
        .unwrap();

        assert_eq!(store.load().unwrap(), vec![wanted]);
        assert_eq!(fs::read(store.path()).unwrap(), backup_bytes);
        assert_eq!(fs::read(store.backup_path()).unwrap(), backup_bytes);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn both_corrupt_returns_primary_error_without_mutating_either_snapshot() {
        let root = dir("both-corrupt");
        let store = FileHistoryStore::new(root.join("history.json"));
        fs::create_dir_all(&root).unwrap();
        let primary_bytes = b"bad primary";
        let backup_bytes = b"bad backup";
        fs::write(store.path(), primary_bytes).unwrap();
        fs::write(store.backup_path(), backup_bytes).unwrap();

        assert!(matches!(store.load(), Err(StoreError::Decode(_))));
        assert_eq!(fs::read(store.path()).unwrap(), primary_bytes);
        assert_eq!(fs::read(store.backup_path()).unwrap(), backup_bytes);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_primary_is_never_rotated_over_backup() {
        let root = dir("rotation");
        let store = FileHistoryStore::new(root.join("history.json"));
        fs::create_dir_all(&root).unwrap();
        fs::write(store.path(), b"bad primary").unwrap();
        fs::write(store.backup_path(), b"known backup").unwrap();
        store.save(&[sample(0, 7)]).unwrap();
        assert_eq!(fs::read(store.backup_path()).unwrap(), b"known backup");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn valid_primary_rotates_byte_exactly_before_replacement() {
        let root = dir("byte-rotation");
        let store = FileHistoryStore::new(root.join("history.json"));
        fs::create_dir_all(&root).unwrap();
        let old = sample(-1, 6);
        let old_bytes = serde_json::to_vec(&vec![old]).unwrap();
        fs::write(store.path(), &old_bytes).unwrap();
        let replacement = sample(0, 7);
        let replacement_bytes = serde_json::to_vec(&vec![replacement.clone()]).unwrap();

        store.save(&[replacement]).unwrap();

        assert_eq!(fs::read(store.backup_path()).unwrap(), old_bytes);
        assert_eq!(fs::read(store.path()).unwrap(), replacement_bytes);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn empty_save_deletes_primary_and_backup() {
        let root = dir("empty");
        let store = FileHistoryStore::new(root.join("history.json"));
        fs::create_dir_all(&root).unwrap();
        fs::write(store.path(), b"[]").unwrap();
        fs::write(store.backup_path(), b"[]").unwrap();
        store.save(&[]).unwrap();
        assert!(!store.path().exists());
        assert!(!store.backup_path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn empty_save_reports_primary_removal_failure_after_removing_backup() {
        let root = dir("empty-primary-failure");
        let store = FileHistoryStore::new(root.join("history.json"));
        fs::create_dir_all(store.path()).unwrap();
        fs::write(store.backup_path(), b"[]").unwrap();

        assert!(matches!(store.save(&[]), Err(StoreError::Io(_))));
        assert!(store.path().is_dir());
        assert!(!store.backup_path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn empty_save_reports_backup_removal_failure_after_removing_primary() {
        let root = dir("empty-backup-failure");
        let store = FileHistoryStore::new(root.join("history.json"));
        fs::create_dir_all(store.backup_path()).unwrap();
        fs::write(store.path(), b"[]").unwrap();

        assert!(matches!(store.save(&[]), Err(StoreError::Io(_))));
        assert!(!store.path().exists());
        assert!(store.backup_path().is_dir());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cap_is_enforced_before_decode() {
        let root = dir("cap");
        let store = FileHistoryStore::new(root.join("history.json"));
        fs::create_dir_all(&root).unwrap();
        let file = File::create(store.path()).unwrap();
        file.set_len(MAX_HISTORY_BYTES + 1).unwrap();
        assert!(matches!(store.load(), Err(StoreError::TooLarge { .. })));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn non_regular_snapshots_are_rejected() {
        use std::os::unix::fs::symlink;

        let root = dir("regular");
        let store = FileHistoryStore::new(root.join("history.json"));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("target.json");
        fs::write(&target, b"[]").unwrap();
        symlink(&target, store.path()).unwrap();
        assert!(matches!(
            store.load(),
            Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::InvalidData
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn coalescer_keeps_records_added_after_first_generation() {
        let root = dir("generation");
        let store = FileHistoryStore::new(root.join("history.json"));
        let writer = HistoryWriteCoalescer::new(store.clone(), Vec::new());
        let first = sample(0, 1);
        writer.record(first.clone()).unwrap();
        for count in 2..=13 {
            writer.record(sample(count, count)).unwrap();
        }
        writer.flush().unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 13);
        assert_eq!(loaded.first(), Some(&first));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_record_interleaving_loses_no_generation() {
        let root = dir("interleaving");
        let store = FileHistoryStore::new(root.join("history.json"));
        let writer = Arc::new(HistoryWriteCoalescer::new(store.clone(), Vec::new()));
        let base = Utc::now().with_nanosecond(0).unwrap() - ChronoDuration::hours(1);
        let mut producers = Vec::new();
        for producer in 0..4_i64 {
            let writer = Arc::clone(&writer);
            producers.push(thread::spawn(move || {
                for offset in 0..8_i64 {
                    let sequence = producer * 8 + offset;
                    writer
                        .record(HistorySample::new(
                            base + ChronoDuration::seconds(sequence),
                            sequence,
                        ))
                        .unwrap();
                    thread::yield_now();
                }
            }));
        }
        for producer in producers {
            producer.join().unwrap();
        }

        writer.flush().unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 32);
        assert_eq!(
            loaded.iter().map(|sample| sample.count).collect::<Vec<_>>(),
            (0..32_i64).collect::<Vec<_>>()
        );
        assert_eq!(
            fs::read(store.path()).unwrap(),
            serde_json::to_vec(&loaded).unwrap()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn first_record_waits_for_a_threshold_or_explicit_flush() {
        let root = dir("coalesced-first");
        let store = FileHistoryStore::new(root.join("history.json"));
        let writer = HistoryWriteCoalescer::new(store.clone(), Vec::new());
        writer.record(sample(0, 1)).unwrap();

        thread::sleep(Duration::from_millis(20));
        assert!(!store.path().exists());

        writer.flush().unwrap();
        assert!(store.path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn snapshot_includes_unflushed_records() {
        let root = dir("snapshot");
        let store = FileHistoryStore::new(root.join("history.json"));
        let initial = sample(-1, 4);
        let pending = sample(0, 7);
        let writer = HistoryWriteCoalescer::new(store.clone(), vec![initial.clone()]);
        writer.record(pending.clone()).unwrap();

        assert_eq!(writer.snapshot().unwrap(), vec![initial, pending]);
        assert!(!store.path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn clear_removes_primary_backup_and_pending_memory() {
        let root = dir("coalescer-clear");
        let path = root.join("history.json");
        let store = FileHistoryStore::new(&path);
        fs::create_dir_all(&root).unwrap();
        store.save(&[sample(0, 1)]).unwrap();
        store.save(&[sample(0, 1), sample(1, 2)]).unwrap();
        assert!(store.backup_path().exists());
        let writer = HistoryWriteCoalescer::new(store.clone(), store.load().unwrap());
        writer.record(sample(2, 3)).unwrap();

        writer.clear().unwrap();

        assert!(writer.snapshot().unwrap().is_empty());
        assert!(!store.path().exists());
        assert!(!store.backup_path().exists());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn twelve_pending_samples_trigger_an_automatic_flush() {
        let root = dir("threshold");
        let store = FileHistoryStore::new(root.join("history.json"));
        let writer = HistoryWriteCoalescer::new(store.clone(), Vec::new());
        writer.record(sample(0, 1)).unwrap();
        writer.flush().unwrap();

        for count in 2..=12 {
            writer.record(sample(count, count)).unwrap();
        }
        thread::sleep(Duration::from_millis(20));
        assert_eq!(store.load().unwrap().len(), 1);

        writer.record(sample(13, 13)).unwrap();
        for _ in 0..100 {
            if store.load().is_ok_and(|samples| samples.len() == 13) {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(store.load().unwrap().len(), 13);
        let _ = fs::remove_dir_all(root);
    }
}
