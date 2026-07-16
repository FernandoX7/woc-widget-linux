//! Live composition root for the always-running application services.

use std::{
    sync::{mpsc::Receiver, Arc, Mutex},
    time::Duration,
};

use crate::{
    community::{CommunityStore, ReleaseMonitor, SystemClock},
    poller::{PollTransport, Poller},
    settings::{Settings, SettingsChange, SettingsStore},
};
use wockit::{
    error::FetchError,
    history::store::{FileHistoryStore, HistoryWriteCoalescer},
    models::{Candle, CandleInterval, CryptoQuote, StatusResponse},
    services::{CommunityService, CryptoService, GeckoTerminalService, StatusService},
};

/// Stable Tauri event names reserved for the Phase 8 tray and Phase 9+ dashboard.
pub mod events {
    pub const STATUS_CHANGED: &str = "woc://status-changed";
    pub const MARKET_CHANGED: &str = "woc://market-changed";
    pub const COMMUNITY_CHANGED: &str = "woc://community-changed";
    pub const SETTINGS_CHANGED: &str = "woc://settings-changed";
    pub const RELEASE_ALERT: &str = "woc://release-alert";
}

pub struct LivePollTransport {
    status: StatusService<wockit::http::ReqwestTransport>,
    quote: CryptoService<wockit::http::ReqwestTransport>,
    candles: GeckoTerminalService<wockit::http::ReqwestTransport>,
}

impl Default for LivePollTransport {
    fn default() -> Self {
        Self {
            status: StatusService::live(),
            quote: CryptoService::live(),
            candles: GeckoTerminalService::new(),
        }
    }
}

impl PollTransport for LivePollTransport {
    async fn status(&self) -> Result<StatusResponse, FetchError> {
        self.status.fetch_status().await
    }

    async fn quote(&self) -> Result<CryptoQuote, FetchError> {
        self.quote.fetch_quote().await
    }

    async fn candles(
        &self,
        interval: CandleInterval,
        count: usize,
    ) -> Result<Vec<Candle>, FetchError> {
        self.candles
            .fetch_candles_with_count(interval, count as u32)
            .await
    }
}

pub type LiveCommunityStore = CommunityStore<CommunityService, SystemClock>;
pub type LiveReleaseMonitor = ReleaseMonitor<CommunityService>;

/// One managed service graph. Consumers subscribe to the individual watch/change streams;
/// no polling or animation is required to observe state changes.
pub struct AppState {
    pub poller: Poller<LivePollTransport>,
    pub community: LiveCommunityStore,
    pub release_monitor: LiveReleaseMonitor,
    pub settings: Arc<Mutex<SettingsStore>>,
    pub history: Arc<HistoryWriteCoalescer>,
    settings_changes: Mutex<Option<Receiver<SettingsChange>>>,
}

impl AppState {
    pub fn open() -> std::io::Result<Self> {
        let mut settings_store = SettingsStore::open()?;
        let settings_changes = settings_store.subscribe();
        let settings: Settings = settings_store.settings().clone();
        let history_store = FileHistoryStore::xdg()?;
        let history = history_store.load().map_err(std::io::Error::other)?;
        Ok(Self {
            poller: Poller::new_with_candle_interval(
                LivePollTransport::default(),
                Duration::from_secs_f64(settings.poll_seconds),
                Duration::from_secs_f64(settings.crypto_poll_seconds),
                candle_interval(settings.crypto_chart_interval),
            ),
            community: CommunityStore::new(CommunityService::new(), SystemClock),
            release_monitor: ReleaseMonitor::new(
                CommunityService::new(),
                settings.release_alerts_enabled,
            ),
            settings: Arc::new(Mutex::new(settings_store)),
            history: Arc::new(HistoryWriteCoalescer::new(history_store, history)),
            settings_changes: Mutex::new(Some(settings_changes)),
        })
    }

    pub fn take_settings_changes(&self) -> Option<Receiver<SettingsChange>> {
        self.settings_changes.lock().unwrap().take()
    }

    pub fn apply_settings(&self, settings: &Settings) -> bool {
        self.poller.set_schedules(
            Duration::from_secs_f64(settings.poll_seconds),
            Duration::from_secs_f64(settings.crypto_poll_seconds),
        );
        self.release_monitor
            .set_enabled(settings.release_alerts_enabled)
    }
}

pub fn candle_interval(seconds: i64) -> CandleInterval {
    CandleInterval::ALL_CASES
        .into_iter()
        .find(|interval| interval.seconds() == seconds)
        .unwrap_or(CandleInterval::FiveMin)
}
