//! Bounded history persistence hooks for shell lifecycle events.

use std::{sync::Arc, time::Duration};
use wockit::history::store::{HistoryWriteCoalescer, StoreError};

pub const FINAL_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

pub trait HistoryFlush: Send + Sync + 'static {
    fn final_drain(&self, timeout: Duration) -> Result<(), StoreError>;
}

impl HistoryFlush for HistoryWriteCoalescer {
    fn final_drain(&self, timeout: Duration) -> Result<(), StoreError> {
        HistoryWriteCoalescer::final_drain(self, timeout)
    }
}

#[derive(Clone)]
pub struct LifecycleFlush {
    history: Arc<dyn HistoryFlush>,
    timeout: Duration,
}

impl LifecycleFlush {
    pub fn new(history: Arc<dyn HistoryFlush>) -> Self {
        Self {
            history,
            timeout: FINAL_FLUSH_TIMEOUT,
        }
    }

    pub fn with_timeout(history: Arc<dyn HistoryFlush>, timeout: Duration) -> Self {
        Self { history, timeout }
    }

    pub fn on_quit(&self) -> Result<(), StoreError> {
        self.drain()
    }
    pub fn on_sigterm(&self) -> Result<(), StoreError> {
        self.drain()
    }
    pub fn on_dashboard_close(&self) -> Result<(), StoreError> {
        self.drain()
    }

    fn drain(&self) -> Result<(), StoreError> {
        self.history.final_drain(self.timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake(Mutex<Vec<Duration>>);
    impl HistoryFlush for Fake {
        fn final_drain(&self, timeout: Duration) -> Result<(), StoreError> {
            self.0.lock().unwrap().push(timeout);
            Ok(())
        }
    }

    #[test]
    fn every_lifecycle_path_uses_the_same_bound() {
        let fake = Arc::new(Fake::default());
        let hooks = LifecycleFlush::with_timeout(fake.clone(), Duration::from_millis(25));
        hooks.on_quit().unwrap();
        hooks.on_sigterm().unwrap();
        hooks.on_dashboard_close().unwrap();
        assert_eq!(*fake.0.lock().unwrap(), vec![Duration::from_millis(25); 3]);
    }

    #[cfg(unix)]
    #[test]
    fn unwritable_history_path_returns_without_hanging_quit() {
        use std::{env, fs, os::unix::fs::PermissionsExt, time::Instant};
        use wockit::history::store::{FileHistoryStore, HistoryWriteCoalescer};

        let root = env::temp_dir().join(format!("woc-dead-history-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let writer = Arc::new(HistoryWriteCoalescer::new(
            FileHistoryStore::new(root.join("history.json")),
            Vec::new(),
        ));
        writer
            .record(wockit::history::HistorySample::new(chrono::Utc::now(), 1))
            .unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();

        let hooks = LifecycleFlush::with_timeout(writer, Duration::from_millis(250));
        let started = Instant::now();
        assert!(hooks.on_quit().is_err());
        assert!(started.elapsed() < Duration::from_secs(1));

        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
