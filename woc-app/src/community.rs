//! Observable Community-page cache and the independent release-alert monitor.

use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime},
};

use tokio::sync::watch;
use wockit::{
    alerts::release::{ReleaseAlertPayload, ReleaseMonitorPolicy},
    models::{LifetimeLeaderboard, ProjectStats, RealmDirectory, ReleaseFeed},
    services::CommunityService,
};

pub const COMMUNITY_CACHE_DURATION: Duration = Duration::from_secs(15 * 60);
pub const COMMUNITY_FAILURE_RETRY_DURATION: Duration = Duration::from_secs(60);
pub const COMMUNITY_RELEASE_LIMIT: i64 = 3;
pub const COMMUNITY_LEADERBOARD_LIMIT: i64 = 5;
pub const RELEASE_MONITOR_INTERVAL: Duration = Duration::from_secs(30 * 60);
pub const RELEASE_MONITOR_TOLERANCE: Duration = Duration::from_secs(3 * 60);
pub const RELEASE_MONITOR_FETCH_LIMIT: i64 = 10;

type FetchFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

/// Injectable async seam. The production implementation delegates to `wockit`'s hardened client.
pub trait CommunityFetching: Send + Sync + 'static {
    fn fetch_project_stats(&self) -> FetchFuture<'_, ProjectStats>;
    fn fetch_releases(&self, limit: i64) -> FetchFuture<'_, ReleaseFeed>;
    fn fetch_leaderboard(&self, limit: i64) -> FetchFuture<'_, LifetimeLeaderboard>;
    fn fetch_realms(&self) -> FetchFuture<'_, RealmDirectory>;
}

impl CommunityFetching for CommunityService {
    fn fetch_project_stats(&self) -> FetchFuture<'_, ProjectStats> {
        Box::pin(async { self.fetch_project_stats().await.map_err(|e| e.to_string()) })
    }
    fn fetch_releases(&self, limit: i64) -> FetchFuture<'_, ReleaseFeed> {
        Box::pin(async move { self.fetch_releases(limit).await.map_err(|e| e.to_string()) })
    }
    fn fetch_leaderboard(&self, limit: i64) -> FetchFuture<'_, LifetimeLeaderboard> {
        Box::pin(async move {
            self.fetch_leaderboard(limit)
                .await
                .map_err(|e| e.to_string())
        })
    }
    fn fetch_realms(&self) -> FetchFuture<'_, RealmDirectory> {
        Box::pin(async { self.fetch_realms().await.map_err(|e| e.to_string()) })
    }
}

pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> SystemTime;
}

#[derive(Debug, Default)]
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedPhase {
    Idle,
    Loading,
    Loaded,
    Cached,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommunityFeed {
    ProjectStats,
    Releases,
    Leaderboard,
    Realms,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunityPhase {
    Idle,
    Loading,
    Loaded,
    Partial,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityFeedState<T> {
    pub value: Option<T>,
    pub last_attempt: Option<SystemTime>,
    pub last_success: Option<SystemTime>,
    pub error: Option<String>,
    pub phase: FeedPhase,
}

impl<T> Default for CommunityFeedState<T> {
    fn default() -> Self {
        Self {
            value: None,
            last_attempt: None,
            last_success: None,
            error: None,
            phase: FeedPhase::Idle,
        }
    }
}

impl<T> CommunityFeedState<T> {
    fn loading(&mut self) {
        self.phase = FeedPhase::Loading;
    }
    fn resolve(&mut self, value: T, at: SystemTime) {
        self.value = Some(value);
        self.last_attempt = Some(at);
        self.last_success = Some(at);
        self.error = None;
        self.phase = FeedPhase::Loaded;
    }
    fn reject(&mut self, error: String, at: SystemTime) {
        self.last_attempt = Some(at);
        self.error = Some(error);
        self.phase = if self.value.is_some() {
            FeedPhase::Cached
        } else {
            FeedPhase::Failed
        };
    }
    fn should_retry(&self, now: SystemTime, cache: Duration, cooldown: Duration) -> bool {
        if self.phase == FeedPhase::Loading {
            return false;
        }
        if self.error.is_some() {
            return elapsed(now, self.last_attempt).is_none_or(|age| age >= cooldown);
        }
        elapsed(now, self.last_success).is_none_or(|age| age >= cache)
    }
}

fn elapsed(now: SystemTime, earlier: Option<SystemTime>) -> Option<Duration> {
    now.duration_since(earlier?).ok()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommunitySnapshot {
    pub project_stats: CommunityFeedState<ProjectStats>,
    pub releases: CommunityFeedState<ReleaseFeed>,
    pub leaderboard: CommunityFeedState<LifetimeLeaderboard>,
    pub realms: CommunityFeedState<RealmDirectory>,
}

/// Four independent feed states, published as immutable snapshots through a watch channel.
pub struct CommunityStore<S, C> {
    service: Arc<S>,
    clock: Arc<C>,
    state: Mutex<CommunitySnapshot>,
    updates: watch::Sender<CommunitySnapshot>,
    in_flight: AtomicBool,
    cache_duration: Duration,
    retry_duration: Duration,
}

impl<S: CommunityFetching, C: Clock> CommunityStore<S, C> {
    pub fn new(service: S, clock: C) -> Self {
        Self::with_durations(
            service,
            clock,
            COMMUNITY_CACHE_DURATION,
            COMMUNITY_FAILURE_RETRY_DURATION,
        )
    }

    pub fn with_durations(service: S, clock: C, cache: Duration, retry: Duration) -> Self {
        let initial = CommunitySnapshot::default();
        let (updates, _) = watch::channel(initial.clone());
        Self {
            service: Arc::new(service),
            clock: Arc::new(clock),
            state: Mutex::new(initial),
            updates,
            in_flight: AtomicBool::new(false),
            cache_duration: cache,
            retry_duration: retry,
        }
    }

    pub fn snapshot(&self) -> CommunitySnapshot {
        self.state.lock().unwrap().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<CommunitySnapshot> {
        self.updates.subscribe()
    }

    pub fn phase(&self) -> CommunityPhase {
        let snapshot = self.snapshot();
        let phases = [
            snapshot.project_stats.phase,
            snapshot.releases.phase,
            snapshot.leaderboard.phase,
            snapshot.realms.phase,
        ];
        let attempts = [
            snapshot.project_stats.last_attempt,
            snapshot.releases.last_attempt,
            snapshot.leaderboard.last_attempt,
            snapshot.realms.last_attempt,
        ];
        let errors = [
            snapshot.project_stats.error.is_some(),
            snapshot.releases.error.is_some(),
            snapshot.leaderboard.error.is_some(),
            snapshot.realms.error.is_some(),
        ];
        let values = [
            snapshot.project_stats.value.is_some(),
            snapshot.releases.value.is_some(),
            snapshot.leaderboard.value.is_some(),
            snapshot.realms.value.is_some(),
        ];
        if phases.contains(&FeedPhase::Loading) {
            return CommunityPhase::Loading;
        }
        if attempts.iter().all(Option::is_none) {
            return CommunityPhase::Idle;
        }
        let failures = errors.into_iter().filter(|failed| *failed).count();
        if failures == 4 {
            CommunityPhase::Failed
        } else if failures > 0 || values.contains(&false) {
            CommunityPhase::Partial
        } else {
            CommunityPhase::Loaded
        }
    }

    pub fn last_success(&self) -> Option<SystemTime> {
        let snapshot = self.snapshot();
        [
            &snapshot.project_stats.last_success,
            &snapshot.releases.last_success,
            &snapshot.leaderboard.last_success,
            &snapshot.realms.last_success,
        ]
        .into_iter()
        .flatten()
        .copied()
        .min()
    }

    pub fn last_attempt(&self) -> Option<SystemTime> {
        let snapshot = self.snapshot();
        [
            &snapshot.project_stats.last_attempt,
            &snapshot.releases.last_attempt,
            &snapshot.leaderboard.last_attempt,
            &snapshot.realms.last_attempt,
        ]
        .into_iter()
        .flatten()
        .copied()
        .max()
    }

    pub async fn refresh_feed(&self, feed: CommunityFeed) -> bool {
        self.refresh_selected(match feed {
            CommunityFeed::ProjectStats => [true, false, false, false],
            CommunityFeed::Releases => [false, true, false, false],
            CommunityFeed::Leaderboard => [false, false, true, false],
            CommunityFeed::Realms => [false, false, false, true],
        })
        .await
    }

    pub async fn retry_failed_feeds(&self) -> bool {
        let snapshot = self.snapshot();
        self.refresh_selected([
            snapshot.project_stats.error.is_some(),
            snapshot.releases.error.is_some(),
            snapshot.leaderboard.error.is_some(),
            snapshot.realms.error.is_some(),
        ])
        .await
    }

    /// Returns false when an overlapping refresh is dropped (never queued).
    pub async fn refresh_if_needed(&self) -> bool {
        let now = self.clock.now();
        let selected = {
            let s = self.state.lock().unwrap();
            [
                s.project_stats
                    .should_retry(now, self.cache_duration, self.retry_duration),
                s.releases
                    .should_retry(now, self.cache_duration, self.retry_duration),
                s.leaderboard
                    .should_retry(now, self.cache_duration, self.retry_duration),
                s.realms
                    .should_retry(now, self.cache_duration, self.retry_duration),
            ]
        };
        self.refresh_selected(selected).await
    }

    pub async fn refresh(&self) -> bool {
        self.refresh_selected([true; 4]).await
    }

    async fn refresh_selected(&self, selected: [bool; 4]) -> bool {
        if !selected.into_iter().any(|v| v) {
            return true;
        }
        if self.in_flight.swap(true, Ordering::AcqRel) {
            return false;
        }
        struct Clear<'a>(&'a AtomicBool);
        impl Drop for Clear<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _clear = Clear(&self.in_flight);
        {
            let mut s = self.state.lock().unwrap();
            if selected[0] {
                s.project_stats.loading();
            }
            if selected[1] {
                s.releases.loading();
            }
            if selected[2] {
                s.leaderboard.loading();
            }
            if selected[3] {
                s.realms.loading();
            }
            self.updates.send_replace(s.clone());
        }
        let service = &self.service;
        let (stats, releases, leaders, realms) = tokio::join!(
            async {
                if selected[0] {
                    Some(service.fetch_project_stats().await)
                } else {
                    None
                }
            },
            async {
                if selected[1] {
                    Some(service.fetch_releases(COMMUNITY_RELEASE_LIMIT).await)
                } else {
                    None
                }
            },
            async {
                if selected[2] {
                    Some(service.fetch_leaderboard(COMMUNITY_LEADERBOARD_LIMIT).await)
                } else {
                    None
                }
            },
            async {
                if selected[3] {
                    Some(service.fetch_realms().await)
                } else {
                    None
                }
            },
        );
        let at = self.clock.now();
        let mut s = self.state.lock().unwrap();
        apply(&mut s.project_stats, stats, at);
        apply(&mut s.releases, releases, at);
        apply(&mut s.leaderboard, leaders, at);
        apply(&mut s.realms, realms, at);
        self.updates.send_replace(s.clone());
        true
    }
}

fn apply<T>(state: &mut CommunityFeedState<T>, result: Option<Result<T, String>>, at: SystemTime) {
    match result {
        Some(Ok(value)) => state.resolve(value, at),
        Some(Err(error)) => state.reject(error, at),
        None => {}
    }
}

/// Independent low-frequency release check. Payloads are returned to orchestration; this layer
/// intentionally performs no notification dispatch.
pub struct ReleaseMonitor<S> {
    service: Arc<S>,
    policy: Mutex<ReleaseMonitorPolicy>,
    in_flight: AtomicBool,
}

impl<S: CommunityFetching> ReleaseMonitor<S> {
    pub fn new(service: S, enabled: bool) -> Self {
        Self {
            service: Arc::new(service),
            policy: Mutex::new(ReleaseMonitorPolicy::new(enabled)),
            in_flight: AtomicBool::new(false),
        }
    }
    pub fn schedule() -> (Duration, Duration) {
        (RELEASE_MONITOR_INTERVAL, RELEASE_MONITOR_TOLERANCE)
    }
    pub fn start(&self) -> bool {
        self.policy.lock().unwrap().start()
    }
    pub fn set_enabled(&self, enabled: bool) -> bool {
        self.policy.lock().unwrap().set_enabled(enabled)
    }

    pub async fn tick(&self) -> Option<ReleaseAlertPayload> {
        if self.in_flight.swap(true, Ordering::AcqRel) {
            return None;
        }
        struct Clear<'a>(&'a AtomicBool);
        impl Drop for Clear<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _clear = Clear(&self.in_flight);
        if !self.policy.lock().unwrap().begin_tick() {
            return None;
        }
        match self
            .service
            .fetch_releases(RELEASE_MONITOR_FETCH_LIMIT)
            .await
        {
            Ok(feed) => self.policy.lock().unwrap().finish_success(&feed.releases),
            Err(_) => {
                self.policy.lock().unwrap().finish_failure();
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    #[derive(Default)]
    struct TestClock(AtomicU64);
    impl TestClock {
        fn advance(&self, seconds: u64) {
            self.0.fetch_add(seconds, Ordering::Relaxed);
        }
    }
    impl Clock for TestClock {
        fn now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_secs(self.0.load(Ordering::Relaxed))
        }
    }

    #[derive(Default)]
    struct Stub {
        calls: Mutex<Vec<(&'static str, i64)>>,
        failures: Mutex<Vec<&'static str>>,
        active: AtomicUsize,
        maximum_active: AtomicUsize,
        release_generation: AtomicU64,
        blocked_feed: Mutex<Option<&'static str>>,
        entered: tokio::sync::Notify,
        release_block: tokio::sync::Notify,
    }

    impl Stub {
        async fn enter<T>(&self, feed: &'static str, limit: i64, value: T) -> Result<T, String> {
            self.calls.lock().unwrap().push((feed, limit));
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum_active.fetch_max(active, Ordering::SeqCst);
            if *self.blocked_feed.lock().unwrap() == Some(feed) {
                self.entered.notify_one();
                self.release_block.notified().await;
            }
            tokio::task::yield_now().await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            if self.failures.lock().unwrap().contains(&feed) {
                Err("unavailable".into())
            } else {
                Ok(value)
            }
        }
        fn release(&self) -> wockit::models::GameRelease {
            let id = self.release_generation.load(Ordering::Relaxed) as i64 + 1;
            serde_json::from_str(&format!(r#"{{"id":{id},"tag":"v{id}"}}"#)).unwrap()
        }
    }

    impl CommunityFetching for Arc<Stub> {
        fn fetch_project_stats(&self) -> FetchFuture<'_, ProjectStats> {
            Box::pin(async {
                self.enter(
                    "stats",
                    0,
                    serde_json::from_str(r#"{"players_online":2}"#).unwrap(),
                )
                .await
            })
        }
        fn fetch_releases(&self, limit: i64) -> FetchFuture<'_, ReleaseFeed> {
            Box::pin(async move {
                let value = ReleaseFeed {
                    repository: None,
                    releases: vec![self.release()],
                };
                self.enter("releases", limit, value).await
            })
        }
        fn fetch_leaderboard(&self, limit: i64) -> FetchFuture<'_, LifetimeLeaderboard> {
            Box::pin(async move {
                self.enter(
                    "leaders",
                    limit,
                    serde_json::from_str(r#"{"leaders":[{"name":"First"}]}"#).unwrap(),
                )
                .await
            })
        }
        fn fetch_realms(&self) -> FetchFuture<'_, RealmDirectory> {
            Box::pin(async {
                self.enter(
                    "realms",
                    0,
                    serde_json::from_str(r#"{"realms":[],"characters":{}}"#).unwrap(),
                )
                .await
            })
        }
    }

    #[tokio::test]
    async fn four_feeds_are_concurrent_observable_and_use_page_limits() {
        let service = Arc::new(Stub::default());
        let store = CommunityStore::new(service.clone(), TestClock::default());
        let mut observer = store.subscribe();
        assert!(store.refresh().await);
        observer.changed().await.unwrap();
        let snapshot = observer.borrow().clone();
        assert_eq!(snapshot.project_stats.phase, FeedPhase::Loaded);
        assert_eq!(snapshot.releases.phase, FeedPhase::Loaded);
        assert!(service.maximum_active.load(Ordering::SeqCst) > 1);
        let calls = service.calls.lock().unwrap().clone();
        assert!(calls.contains(&("releases", COMMUNITY_RELEASE_LIMIT)));
        assert!(calls.contains(&("leaders", COMMUNITY_LEADERBOARD_LIMIT)));
    }

    #[tokio::test]
    async fn cache_boundary_and_failed_feed_cooldown_are_per_feed() {
        let service = Arc::new(Stub::default());
        let clock = TestClock::default();
        let store = CommunityStore::new(service.clone(), clock);
        store.refresh().await;
        assert!(store.refresh_if_needed().await);
        assert_eq!(service.calls.lock().unwrap().len(), 4);

        store.clock.advance(900);
        service.failures.lock().unwrap().push("leaders");
        store.refresh_if_needed().await;
        assert_eq!(store.snapshot().leaderboard.phase, FeedPhase::Cached);
        service.failures.lock().unwrap().clear();
        store.refresh_if_needed().await;
        assert_eq!(service.calls.lock().unwrap().len(), 8);

        store.clock.advance(59);
        store.refresh_if_needed().await;
        assert_eq!(service.calls.lock().unwrap().len(), 8);
        store.clock.advance(1);
        store.refresh_if_needed().await;
        assert_eq!(service.calls.lock().unwrap().len(), 9);
        assert_eq!(store.snapshot().leaderboard.phase, FeedPhase::Loaded);
    }

    #[tokio::test]
    async fn explicit_retry_targets_only_failed_feeds_and_aggregate_is_truthful() {
        let service = Arc::new(Stub::default());
        let store = CommunityStore::new(service.clone(), TestClock::default());
        service.failures.lock().unwrap().push("leaders");
        store.refresh().await;
        assert_eq!(store.phase(), CommunityPhase::Partial);
        assert!(store.last_success().is_some());
        assert!(store.last_attempt().is_some());
        service.failures.lock().unwrap().clear();
        assert!(store.retry_failed_feeds().await);
        assert_eq!(service.calls.lock().unwrap().len(), 5);
        assert_eq!(store.phase(), CommunityPhase::Loaded);
        assert!(store.refresh_feed(CommunityFeed::Releases).await);
        assert_eq!(service.calls.lock().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn release_monitor_is_baseline_safe_independent_and_requests_ten() {
        let service = Arc::new(Stub::default());
        let monitor = ReleaseMonitor::new(service.clone(), true);
        assert_eq!(
            ReleaseMonitor::<Arc<Stub>>::schedule(),
            (Duration::from_secs(1800), Duration::from_secs(180))
        );
        assert!(monitor.start());
        assert_eq!(monitor.tick().await, None);
        service.release_generation.store(1, Ordering::Relaxed);
        assert_eq!(monitor.tick().await.unwrap().identity, "id:2");
        let release_limits: Vec<_> = service
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| *name == "releases")
            .map(|(_, limit)| *limit)
            .collect();
        assert_eq!(release_limits, [10, 10]);
    }

    #[tokio::test]
    async fn release_monitor_drops_an_overlapping_tick() {
        let service = Arc::new(Stub::default());
        *service.blocked_feed.lock().unwrap() = Some("releases");
        let monitor = Arc::new(ReleaseMonitor::new(service.clone(), true));
        assert!(monitor.start());
        let running = {
            let monitor = monitor.clone();
            tokio::spawn(async move { monitor.tick().await })
        };
        service.entered.notified().await;
        assert_eq!(monitor.tick().await, None);
        assert_eq!(service.calls.lock().unwrap().len(), 1);
        service.release_block.notify_one();
        assert_eq!(running.await.unwrap(), None);
    }
}
