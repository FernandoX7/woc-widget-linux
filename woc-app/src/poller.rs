//! Independent status and market-feed orchestration.
//!
//! Network services remain one-shot operations in `wockit`; this module owns cadence,
//! overlap suppression, dashboard visibility, freshness, and truthful feed provenance.

use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

use tokio::sync::{watch, Mutex};
use wockit::{
    config,
    error::{FetchError, StatusFailureKind},
    models::{Candle, CandleInterval, CryptoQuote, StatusResponse},
};

const STATUS_OPEN_FRESHNESS: Duration = Duration::from_secs(20);
const CANDLE_FRESHNESS: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataFeedState {
    Idle,
    Loading,
    Live,
    Cached,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealmAvailability {
    Loading,
    Healthy,
    ServerReportedDown,
    Unreachable(StatusFailureKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerSchedule {
    pub interval: Duration,
    /// Scheduling slack exposed to platform timer integrations. Tokio's interval has no
    /// tolerance knob, so this remains explicit metadata instead of changing cadence.
    pub tolerance: Duration,
}

impl TimerSchedule {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            tolerance: interval.mul_f64(config::poll::TIMER_TOLERANCE_FRACTION),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Feed<T> {
    pub value: Option<T>,
    pub state: DataFeedState,
    pub last_attempt: Option<tokio::time::Instant>,
    pub last_success: Option<tokio::time::Instant>,
    pub error: Option<String>,
    pub is_refreshing: bool,
}

impl<T> Default for Feed<T> {
    fn default() -> Self {
        Self {
            value: None,
            state: DataFeedState::Idle,
            last_attempt: None,
            last_success: None,
            error: None,
            is_refreshing: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PollerSnapshot {
    pub status: Feed<StatusResponse>,
    pub realm_availability: RealmAvailability,
    pub quote: Feed<CryptoQuote>,
    pub candles: Feed<Vec<Candle>>,
    pub selected_candle_interval: CandleInterval,
    pub loaded_candle_interval: Option<CandleInterval>,
    pub dashboard_visible: bool,
}

impl Default for PollerSnapshot {
    fn default() -> Self {
        Self {
            status: Feed::default(),
            realm_availability: RealmAvailability::Loading,
            quote: Feed::default(),
            candles: Feed::default(),
            selected_candle_interval: CandleInterval::FiveMin,
            loaded_candle_interval: None,
            dashboard_visible: false,
        }
    }
}

pub trait PollTransport: Send + Sync + 'static {
    fn status(&self) -> impl Future<Output = Result<StatusResponse, FetchError>> + Send;
    fn quote(&self) -> impl Future<Output = Result<CryptoQuote, FetchError>> + Send;
    fn candles(
        &self,
        interval: CandleInterval,
        count: usize,
    ) -> impl Future<Output = Result<Vec<Candle>, FetchError>> + Send;
}

struct Guards {
    status: AtomicBool,
    quote: AtomicBool,
    candles: AtomicBool,
}

impl Default for Guards {
    fn default() -> Self {
        Self {
            status: AtomicBool::new(false),
            quote: AtomicBool::new(false),
            candles: AtomicBool::new(false),
        }
    }
}

struct InFlight<'a>(&'a AtomicBool);
impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

struct Inner<T> {
    transport: T,
    state: Mutex<PollerSnapshot>,
    guards: Guards,
    updates: watch::Sender<PollerSnapshot>,
    status_schedule: RwLock<TimerSchedule>,
    crypto_schedule: RwLock<TimerSchedule>,
    status_schedule_updates: watch::Sender<TimerSchedule>,
    crypto_schedule_updates: watch::Sender<TimerSchedule>,
}

pub struct Poller<T>(Arc<Inner<T>>);

impl<T> Clone for Poller<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: PollTransport> Poller<T> {
    pub fn new(transport: T, status_interval: Duration, crypto_interval: Duration) -> Self {
        Self::new_with_candle_interval(
            transport,
            status_interval,
            crypto_interval,
            CandleInterval::FiveMin,
        )
    }

    pub fn new_with_candle_interval(
        transport: T,
        status_interval: Duration,
        crypto_interval: Duration,
        candle_interval: CandleInterval,
    ) -> Self {
        let initial = PollerSnapshot {
            selected_candle_interval: candle_interval,
            ..PollerSnapshot::default()
        };
        let (updates, _) = watch::channel(initial.clone());
        let status_schedule = TimerSchedule::new(status_interval);
        let crypto_schedule = TimerSchedule::new(crypto_interval);
        let (status_schedule_updates, _) = watch::channel(status_schedule);
        let (crypto_schedule_updates, _) = watch::channel(crypto_schedule);
        Self(Arc::new(Inner {
            transport,
            state: Mutex::new(initial),
            guards: Guards::default(),
            updates,
            status_schedule: RwLock::new(status_schedule),
            crypto_schedule: RwLock::new(crypto_schedule),
            status_schedule_updates,
            crypto_schedule_updates,
        }))
    }

    pub fn subscribe(&self) -> watch::Receiver<PollerSnapshot> {
        self.0.updates.subscribe()
    }

    pub async fn snapshot(&self) -> PollerSnapshot {
        let mut snapshot = self.0.state.lock().await.clone();
        reconcile_market_states(
            &mut snapshot,
            self.price_freshness(),
            tokio::time::Instant::now(),
        );
        snapshot
    }

    pub fn status_schedule(&self) -> TimerSchedule {
        *self.0.status_schedule.read().unwrap()
    }

    pub fn crypto_schedule(&self) -> TimerSchedule {
        *self.0.crypto_schedule.read().unwrap()
    }

    pub fn set_schedules(&self, status_interval: Duration, crypto_interval: Duration) {
        let status = TimerSchedule::new(status_interval);
        let crypto = TimerSchedule::new(crypto_interval);
        *self.0.status_schedule.write().unwrap() = status;
        *self.0.crypto_schedule.write().unwrap() = crypto;
        self.0.status_schedule_updates.send_replace(status);
        self.0.crypto_schedule_updates.send_replace(crypto);
    }

    /// Starts independent repeating tasks. Each tick invokes an overlap-dropping refresh;
    /// missed work is never queued and there is no retry/backoff loop.
    pub fn start(
        &self,
    ) -> (
        tokio::task::JoinHandle<()>,
        tokio::task::JoinHandle<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let status = self.clone();
        let status_task = tokio::spawn(async move {
            let mut schedules = status.0.status_schedule_updates.subscribe();
            status.refresh_status().await;
            loop {
                let interval = schedules.borrow().interval;
                tokio::select! {
                    _ = tokio::time::sleep(interval) => status.refresh_status().await,
                    changed = schedules.changed() => if changed.is_err() { break },
                }
            }
        });
        let market = self.clone();
        let crypto_task = tokio::spawn(async move {
            let mut schedules = market.0.crypto_schedule_updates.subscribe();
            market.refresh_crypto_tick().await;
            loop {
                let interval = schedules.borrow().interval;
                tokio::select! {
                    _ = tokio::time::sleep(interval) => market.refresh_crypto_tick().await,
                    changed = schedules.changed() => if changed.is_err() { break },
                }
            }
        });
        let freshness = self.clone();
        let freshness_task = tokio::spawn(async move {
            let mut updates = freshness.0.updates.subscribe();
            let mut schedules = freshness.0.crypto_schedule_updates.subscribe();
            loop {
                let live_success = {
                    let state = freshness.0.state.lock().await;
                    (state.quote.state == DataFeedState::Live)
                        .then_some(state.quote.last_success)
                        .flatten()
                };
                let Some(success) = live_success else {
                    tokio::select! {
                        changed = updates.changed() => if changed.is_err() { break },
                        changed = schedules.changed() => {
                            if changed.is_err() { break }
                            freshness.publish().await;
                        },
                    }
                    continue;
                };
                let deadline = success + freshness.price_freshness();
                tokio::select! {
                    _ = tokio::time::sleep_until(deadline) => freshness.publish().await,
                    changed = updates.changed() => if changed.is_err() { break },
                    changed = schedules.changed() => {
                        if changed.is_err() { break }
                        freshness.publish().await;
                    },
                }
            }
        });
        (status_task, crypto_task, freshness_task)
    }

    async fn publish(&self) {
        let snapshot = {
            let mut state = self.0.state.lock().await;
            reconcile_market_states(
                &mut state,
                self.price_freshness(),
                tokio::time::Instant::now(),
            );
            state.clone()
        };
        self.0.updates.send_replace(snapshot);
    }

    fn take_guard(&self, select: impl FnOnce(&Guards) -> &AtomicBool) -> Option<InFlight<'_>> {
        let guard = select(&self.0.guards);
        (!guard.swap(true, Ordering::AcqRel)).then_some(InFlight(guard))
    }

    pub async fn refresh_status(&self) {
        let Some(_guard) = self.take_guard(|g| &g.status) else {
            return;
        };
        {
            let mut state = self.0.state.lock().await;
            state.status.is_refreshing = true;
            if state.status.value.is_none() {
                state.status.state = DataFeedState::Loading;
            }
        }
        self.publish().await;
        let result = self.0.transport.status().await;
        let now = tokio::time::Instant::now();
        {
            let mut state = self.0.state.lock().await;
            state.status.last_attempt = Some(now);
            state.status.is_refreshing = false;
            match result {
                Ok(value) => {
                    state.realm_availability = if value.ok {
                        RealmAvailability::Healthy
                    } else {
                        RealmAvailability::ServerReportedDown
                    };
                    state.status.value = Some(value);
                    state.status.last_success = Some(now);
                    state.status.error = None;
                    state.status.state = DataFeedState::Live;
                }
                Err(error) => {
                    state.realm_availability =
                        RealmAvailability::Unreachable(error.status_failure_kind());
                    state.status.error = Some(error.friendly_message());
                    state.status.state = if state.status.value.is_some() {
                        DataFeedState::Cached
                    } else {
                        DataFeedState::Unavailable
                    };
                }
            }
        }
        self.publish().await;
    }

    pub async fn refresh_quote(&self) {
        let Some(_guard) = self.take_guard(|g| &g.quote) else {
            return;
        };
        {
            let mut state = self.0.state.lock().await;
            state.quote.is_refreshing = true;
            if state.quote.value.is_none() {
                state.quote.state = DataFeedState::Loading;
            }
        }
        self.publish().await;
        let result = self.0.transport.quote().await;
        let now = tokio::time::Instant::now();
        {
            let mut state = self.0.state.lock().await;
            state.quote.last_attempt = Some(now);
            state.quote.is_refreshing = false;
            match result {
                Ok(value) if sanitized_quote(value.clone()).is_some() => {
                    let value = sanitized_quote(value).expect("validated above");
                    state.quote.value = Some(value);
                    state.quote.last_success = Some(now);
                    state.quote.error = None;
                    state.quote.state = DataFeedState::Live;
                }
                Ok(_) | Err(_) => {
                    let message = match result {
                        Err(error) => error.friendly_message(),
                        Ok(_) => "Couldn't update market price".to_owned(),
                    };
                    state.quote.error = Some(message);
                    state.quote.state = if state.quote.value.is_some() {
                        DataFeedState::Cached
                    } else {
                        DataFeedState::Unavailable
                    };
                }
            }
        }
        self.publish().await;
    }

    pub async fn refresh_crypto_tick(&self) {
        let candle_due = {
            let state = self.0.state.lock().await;
            state.dashboard_visible && candles_need_refresh(&state, tokio::time::Instant::now())
        };
        if candle_due {
            tokio::join!(self.refresh_quote(), self.refresh_candles());
        } else {
            self.refresh_quote().await;
        }
    }

    pub async fn refresh_candles(&self) {
        let Some(_guard) = self.take_guard(|g| &g.candles) else {
            return;
        };
        {
            let mut state = self.0.state.lock().await;
            state.candles.is_refreshing = true;
            if state.candles.value.is_none() {
                state.candles.state = DataFeedState::Loading;
            }
        }
        self.publish().await;
        loop {
            let requested = self.0.state.lock().await.selected_candle_interval;
            let result = self
                .0
                .transport
                .candles(requested, config::crypto::CANDLE_COUNT)
                .await;
            let now = tokio::time::Instant::now();
            let mut state = self.0.state.lock().await;
            state.candles.last_attempt = Some(now);
            match result {
                Ok(values) => {
                    let values = normalized_candles(values);
                    if values.is_empty() {
                        candle_failure(&mut state, "Couldn't update chart".to_owned());
                    } else {
                        state.candles.value = Some(values);
                        state.loaded_candle_interval = Some(requested);
                        state.candles.last_success = Some(now);
                        state.candles.error = None;
                        state.candles.state = DataFeedState::Live;
                    }
                }
                Err(error) => candle_failure(&mut state, error.friendly_message()),
            }
            if requested == state.selected_candle_interval {
                break;
            }
            if !state.dashboard_visible {
                break;
            }
        }
        self.0.state.lock().await.candles.is_refreshing = false;
        self.publish().await;
    }

    pub async fn set_dashboard_visible(&self, visible: bool) {
        let should_refresh = {
            let mut state = self.0.state.lock().await;
            if state.dashboard_visible == visible {
                return;
            }
            state.dashboard_visible = visible;
            visible && candles_need_refresh(&state, tokio::time::Instant::now())
        };
        self.publish().await;
        if should_refresh {
            self.refresh_candles().await;
        }
    }

    pub async fn set_candle_interval(&self, interval: CandleInterval) {
        let changed = {
            let mut state = self.0.state.lock().await;
            let changed = state.selected_candle_interval != interval;
            state.selected_candle_interval = interval;
            if changed && state.candles.value.is_some() {
                state.candles.state = DataFeedState::Cached;
            }
            changed
        };
        self.publish().await;
        let visible = self.0.state.lock().await.dashboard_visible;
        if changed && visible {
            self.refresh_candles().await;
        }
    }

    pub async fn refresh_visible_content(&self) {
        let now = tokio::time::Instant::now();
        let (status, quote, candles) = {
            let state = self.0.state.lock().await;
            (
                stale_strict(state.status.last_success, STATUS_OPEN_FRESHNESS, now),
                state.quote.error.is_some()
                    || stale_at_boundary(state.quote.last_success, self.price_freshness(), now),
                state.dashboard_visible && candles_need_refresh(&state, now),
            )
        };
        tokio::join!(
            async {
                if status {
                    self.refresh_status().await
                }
            },
            async {
                if quote {
                    self.refresh_quote().await
                }
            },
            async {
                if candles {
                    self.refresh_candles().await
                }
            },
        );
    }

    pub fn price_freshness(&self) -> Duration {
        config::panel_price_freshness(self.crypto_schedule().interval)
    }
}

fn stale_strict(
    last: Option<tokio::time::Instant>,
    window: Duration,
    now: tokio::time::Instant,
) -> bool {
    last.is_none_or(|stamp| {
        now.checked_duration_since(stamp)
            .is_none_or(|age| age > window)
    })
}

fn stale_at_boundary(
    last: Option<tokio::time::Instant>,
    window: Duration,
    now: tokio::time::Instant,
) -> bool {
    last.is_none_or(|stamp| {
        now.checked_duration_since(stamp)
            .is_none_or(|age| age >= window)
    })
}

fn candles_need_refresh(state: &PollerSnapshot, now: tokio::time::Instant) -> bool {
    state.loaded_candle_interval != Some(state.selected_candle_interval)
        || stale_at_boundary(state.candles.last_attempt, CANDLE_FRESHNESS, now)
}

fn sanitized_quote(mut quote: CryptoQuote) -> Option<CryptoQuote> {
    let valid = quote
        .price
        .parse::<f64>()
        .is_ok_and(|price| price.is_finite() && price > 0.0)
        && quote.change24h.is_finite();
    if !valid {
        return None;
    }
    for window in quote.market.values_mut() {
        window.change_percent = window.change_percent.filter(|value| value.is_finite());
        window.buys = window.buys.filter(|value| *value >= 0);
        window.sells = window.sells.filter(|value| *value >= 0);
        window.volume_usd = window
            .volume_usd
            .filter(|value| value.is_finite() && *value >= 0.0);
    }
    quote.liquidity_usd = quote
        .liquidity_usd
        .filter(|value| value.is_finite() && *value >= 0.0);
    quote.fdv_usd = quote
        .fdv_usd
        .filter(|value| value.is_finite() && *value >= 0.0);
    quote.market_cap_usd = quote
        .market_cap_usd
        .filter(|value| value.is_finite() && *value >= 0.0);
    quote.pair_url =
        config::api::validated_market_url(quote.pair_url.as_ref().map(|url| url.as_str()));
    Some(quote)
}

fn reconcile_market_states(
    state: &mut PollerSnapshot,
    price_window: Duration,
    now: tokio::time::Instant,
) {
    if state.quote.state != DataFeedState::Loading {
        state.quote.state = if state.quote.error.is_some() {
            if state.quote.value.is_some() {
                DataFeedState::Cached
            } else {
                DataFeedState::Unavailable
            }
        } else if state.quote.last_success.is_some() {
            if stale_at_boundary(state.quote.last_success, price_window, now) {
                DataFeedState::Cached
            } else {
                DataFeedState::Live
            }
        } else {
            DataFeedState::Idle
        };
    }
    if state.candles.state != DataFeedState::Loading {
        state.candles.state = if state.candles.error.is_some()
            || (state.candles.value.is_some()
                && state.loaded_candle_interval != Some(state.selected_candle_interval))
        {
            if state.candles.value.is_some() {
                DataFeedState::Cached
            } else {
                DataFeedState::Unavailable
            }
        } else if state.candles.last_success.is_some() {
            if candles_need_refresh(state, now) {
                DataFeedState::Cached
            } else {
                DataFeedState::Live
            }
        } else {
            DataFeedState::Idle
        };
    }
}

fn candle_failure(state: &mut PollerSnapshot, message: String) {
    state.candles.error = Some(message);
    state.candles.state = if state.candles.value.is_some() {
        DataFeedState::Cached
    } else {
        DataFeedState::Unavailable
    };
}

fn normalized_candles(input: Vec<Candle>) -> Vec<Candle> {
    let mut output = Vec::new();
    for candle in input.into_iter().filter(Candle::is_valid) {
        if !output
            .iter()
            .any(|existing: &Candle| existing.date == candle.date)
        {
            output.push(candle);
        }
    }
    output.sort_by_key(|candle| candle.date);
    output
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Mutex as StdMutex,
        },
    };

    use tokio::sync::Notify;
    use wockit::{
        error::TransportErrorKind,
        models::{CryptoMarketTimeframe, CryptoMarketWindow, CryptoQuote},
    };

    use super::*;

    #[derive(Default)]
    struct FakeTransport {
        status_calls: AtomicUsize,
        quote_calls: AtomicUsize,
        candle_calls: AtomicUsize,
        status_blocked: AtomicBool,
        candle_blocked: AtomicBool,
        status_release: Notify,
        candle_release: Notify,
        candle_intervals: StdMutex<Vec<CandleInterval>>,
        status_fails: AtomicBool,
        quote_fails: AtomicBool,
        quote_blocked: AtomicBool,
        quote_release: Notify,
        candle_fails: AtomicBool,
    }

    impl FakeTransport {
        fn quote_value() -> CryptoQuote {
            CryptoQuote {
                price: "1.25".to_owned(),
                change24h: 2.0,
                market: HashMap::new(),
                liquidity_usd: None,
                fdv_usd: None,
                market_cap_usd: None,
                pair_url: None,
            }
        }

        fn candle(_interval: CandleInterval) -> Candle {
            Candle::new(
                "2023-11-14T22:13:20Z".parse().unwrap(),
                1.0,
                2.0,
                0.5,
                1.5,
                3.0,
            )
        }
    }

    impl PollTransport for FakeTransport {
        async fn status(&self) -> Result<StatusResponse, FetchError> {
            self.status_calls.fetch_add(1, Ordering::SeqCst);
            if self.status_blocked.load(Ordering::SeqCst) {
                self.status_release.notified().await;
            }
            if self.status_fails.load(Ordering::SeqCst) {
                Err(FetchError::Transport(TransportErrorKind::TimedOut))
            } else {
                Ok(StatusResponse::new(true, Some("R".to_owned()), 7, None))
            }
        }

        async fn quote(&self) -> Result<CryptoQuote, FetchError> {
            self.quote_calls.fetch_add(1, Ordering::SeqCst);
            if self.quote_blocked.load(Ordering::SeqCst) {
                self.quote_release.notified().await;
            }
            if self.quote_fails.load(Ordering::SeqCst) {
                Err(FetchError::Decode)
            } else {
                Ok(Self::quote_value())
            }
        }

        async fn candles(
            &self,
            interval: CandleInterval,
            count: usize,
        ) -> Result<Vec<Candle>, FetchError> {
            assert_eq!(count, config::crypto::CANDLE_COUNT);
            let call = self.candle_calls.fetch_add(1, Ordering::SeqCst);
            self.candle_intervals.lock().unwrap().push(interval);
            if call == 0 && self.candle_blocked.load(Ordering::SeqCst) {
                self.candle_release.notified().await;
            }
            if self.candle_fails.load(Ordering::SeqCst) {
                Err(FetchError::Decode)
            } else {
                Ok(vec![Self::candle(interval)])
            }
        }
    }

    fn poller(transport: FakeTransport) -> Poller<FakeTransport> {
        Poller::new(transport, Duration::from_secs(60), Duration::from_secs(60))
    }

    #[tokio::test]
    async fn schedule_records_ten_percent_tolerance_and_independent_cadences() {
        let poller = Poller::new(
            FakeTransport::default(),
            Duration::from_secs(10),
            Duration::from_secs(300),
        );
        assert_eq!(poller.status_schedule().tolerance, Duration::from_secs(1));
        assert_eq!(poller.crypto_schedule().tolerance, Duration::from_secs(30));
        assert_ne!(poller.status_schedule(), poller.crypto_schedule());
    }

    #[tokio::test]
    async fn same_feed_overlap_is_dropped_but_other_feed_remains_independent() {
        let transport = FakeTransport::default();
        transport.status_blocked.store(true, Ordering::SeqCst);
        let poller = poller(transport);
        let first = tokio::spawn({
            let poller = poller.clone();
            async move { poller.refresh_status().await }
        });
        tokio::task::yield_now().await;
        poller.refresh_status().await;
        poller.refresh_quote().await;
        assert_eq!(poller.0.transport.status_calls.load(Ordering::SeqCst), 1);
        assert_eq!(poller.0.transport.quote_calls.load(Ordering::SeqCst), 1);
        poller.0.transport.status_release.notify_one();
        first.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_quote_publishes_cached_at_the_shared_deadline_without_an_api_retry() {
        let transport = FakeTransport::default();
        let poller = poller(transport);
        poller.refresh_quote().await;
        let mut updates = poller.subscribe();
        poller
            .0
            .transport
            .quote_blocked
            .store(true, Ordering::SeqCst);
        let tasks = poller.start();
        while poller.0.transport.quote_calls.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }

        tokio::time::advance(poller.price_freshness() - Duration::from_nanos(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(updates.borrow_and_update().quote.state, DataFeedState::Live);
        tokio::time::advance(Duration::from_nanos(1)).await;
        updates.changed().await.unwrap();
        assert_eq!(
            updates.borrow_and_update().quote.state,
            DataFeedState::Cached
        );
        assert_eq!(poller.0.transport.quote_calls.load(Ordering::SeqCst), 2);

        poller.0.transport.quote_release.notify_one();
        tasks.0.abort();
        tasks.1.abort();
        tasks.2.abort();
    }

    #[tokio::test]
    async fn failures_keep_cached_values_and_market_never_changes_realm() {
        let poller = poller(FakeTransport::default());
        poller.refresh_status().await;
        poller.refresh_quote().await;
        poller.set_dashboard_visible(true).await;
        let healthy = poller.snapshot().await.realm_availability;

        poller.0.transport.quote_fails.store(true, Ordering::SeqCst);
        poller
            .0
            .transport
            .candle_fails
            .store(true, Ordering::SeqCst);
        poller.refresh_quote().await;
        poller.refresh_candles().await;
        let state = poller.snapshot().await;
        assert_eq!(state.realm_availability, healthy);
        assert_eq!(state.quote.state, DataFeedState::Cached);
        assert_eq!(state.candles.state, DataFeedState::Cached);

        poller
            .0
            .transport
            .status_fails
            .store(true, Ordering::SeqCst);
        poller.refresh_status().await;
        let state = poller.snapshot().await;
        assert_eq!(
            state.realm_availability,
            RealmAvailability::Unreachable(StatusFailureKind::TimedOut)
        );
        assert_eq!(state.status.state, DataFeedState::Cached);
    }

    #[tokio::test(start_paused = true)]
    async fn visible_open_refreshes_each_feed_at_its_own_exact_boundary() {
        let poller = poller(FakeTransport::default());
        poller.refresh_status().await;
        poller.refresh_quote().await;
        poller.set_dashboard_visible(true).await;
        assert_eq!(poller.0.transport.candle_calls.load(Ordering::SeqCst), 1);

        tokio::time::advance(STATUS_OPEN_FRESHNESS).await;
        poller.refresh_visible_content().await;
        assert_eq!(poller.0.transport.status_calls.load(Ordering::SeqCst), 1);

        tokio::time::advance(Duration::from_secs(1)).await;
        poller.refresh_visible_content().await;
        assert_eq!(poller.0.transport.status_calls.load(Ordering::SeqCst), 2);

        let quote_before = poller.0.transport.quote_calls.load(Ordering::SeqCst);
        tokio::time::advance(poller.price_freshness() - Duration::from_secs(21)).await;
        poller.refresh_visible_content().await;
        assert_eq!(
            poller.0.transport.quote_calls.load(Ordering::SeqCst),
            quote_before + 1
        );

        poller.set_dashboard_visible(false).await;
        tokio::time::advance(CANDLE_FRESHNESS).await;
        poller.refresh_crypto_tick().await;
        assert_eq!(poller.0.transport.candle_calls.load(Ordering::SeqCst), 1);
        poller.set_dashboard_visible(true).await;
        assert_eq!(poller.0.transport.candle_calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn interval_change_during_fetch_trailing_coalesces_to_latest_selection() {
        let transport = FakeTransport::default();
        transport.candle_blocked.store(true, Ordering::SeqCst);
        let poller = poller(transport);
        poller.0.state.lock().await.dashboard_visible = true;
        let fetch = tokio::spawn({
            let poller = poller.clone();
            async move { poller.refresh_candles().await }
        });
        tokio::task::yield_now().await;
        poller.set_candle_interval(CandleInterval::OneHour).await;
        poller.set_candle_interval(CandleInterval::FourHour).await;
        poller.0.transport.candle_release.notify_one();
        fetch.await.unwrap();

        let intervals = poller.0.transport.candle_intervals.lock().unwrap().clone();
        assert_eq!(intervals.first(), Some(&CandleInterval::FiveMin));
        assert_eq!(intervals.last(), Some(&CandleInterval::FourHour));
        assert!(!intervals.contains(&CandleInterval::OneHour));
        let state = poller.snapshot().await;
        assert_eq!(state.loaded_candle_interval, Some(CandleInterval::FourHour));
        assert_eq!(state.candles.state, DataFeedState::Live);
    }

    #[tokio::test]
    async fn hide_and_change_during_fetch_defers_trailing_request_until_reopened() {
        let transport = FakeTransport::default();
        transport.candle_blocked.store(true, Ordering::SeqCst);
        let poller = poller(transport);
        poller.0.state.lock().await.dashboard_visible = true;
        let running = {
            let poller = poller.clone();
            tokio::spawn(async move { poller.refresh_candles().await })
        };
        while poller.0.transport.candle_calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        poller.set_dashboard_visible(false).await;
        poller.set_candle_interval(CandleInterval::FourHour).await;
        poller.0.transport.candle_release.notify_one();
        running.await.unwrap();

        let state = poller.snapshot().await;
        assert_eq!(poller.0.transport.candle_calls.load(Ordering::SeqCst), 1);
        assert_eq!(state.selected_candle_interval, CandleInterval::FourHour);
        assert_eq!(state.loaded_candle_interval, Some(CandleInterval::FiveMin));
        assert_eq!(state.candles.state, DataFeedState::Cached);
        assert!(!state.candles.is_refreshing);

        poller
            .0
            .transport
            .candle_blocked
            .store(false, Ordering::SeqCst);
        poller.set_dashboard_visible(true).await;
        let state = poller.snapshot().await;
        assert_eq!(poller.0.transport.candle_calls.load(Ordering::SeqCst), 2);
        assert_eq!(state.loaded_candle_interval, Some(CandleInterval::FourHour));
        assert_eq!(state.candles.state, DataFeedState::Live);
    }

    #[tokio::test]
    async fn hidden_interval_change_defers_until_visible() {
        let poller = poller(FakeTransport::default());
        poller.set_candle_interval(CandleInterval::OneHour).await;
        assert_eq!(poller.0.transport.candle_calls.load(Ordering::SeqCst), 0);
        poller.set_dashboard_visible(true).await;
        assert_eq!(poller.0.transport.candle_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn quote_sanitization_keeps_spot_and_drops_bad_metadata() {
        let mut quote = FakeTransport::quote_value();
        quote.market.insert(
            CryptoMarketTimeframe::OneHour,
            CryptoMarketWindow {
                change_percent: Some(f64::NAN),
                buys: Some(-1),
                sells: Some(2),
                volume_usd: Some(-3.0),
            },
        );
        quote.liquidity_usd = Some(f64::INFINITY);
        quote.pair_url = Some("https://evil.example/pair".parse().unwrap());
        let sanitized = sanitized_quote(quote).unwrap();
        let window = sanitized
            .market
            .get(&CryptoMarketTimeframe::OneHour)
            .unwrap();
        assert_eq!(window.change_percent, None);
        assert_eq!(window.buys, None);
        assert_eq!(window.sells, Some(2));
        assert_eq!(window.volume_usd, None);
        assert_eq!(sanitized.liquidity_usd, None);
        assert_eq!(sanitized.pair_url, None);
    }

    #[tokio::test]
    async fn cancellation_releases_the_in_flight_guard() {
        let transport = FakeTransport::default();
        transport.status_blocked.store(true, Ordering::SeqCst);
        let poller = poller(transport);
        let task = tokio::spawn({
            let poller = poller.clone();
            async move { poller.refresh_status().await }
        });
        tokio::task::yield_now().await;
        task.abort();
        let _ = task.await;
        poller
            .0
            .transport
            .status_blocked
            .store(false, Ordering::SeqCst);
        poller.refresh_status().await;
        assert_eq!(poller.0.transport.status_calls.load(Ordering::SeqCst), 2);
    }
}
