//! Persistent application settings with the macOS-compatible frozen key names.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    env, fs, io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    time::{SystemTime, UNIX_EPOCH},
};
use wockit::{
    alerts::policy::minute_of_day,
    config::{advanced_alert, crypto_alert, poll, PollInterval},
};

/// Every persisted settings key. Raw values are frozen for cross-platform compatibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DefaultsKey {
    AlertsEnabled,
    PeakAlertsEnabled,
    CryptoAlertsEnabled,
    TokenChangeGainAlertsEnabled,
    TokenChangeLossAlertsEnabled,
    ChartInterval,
    ChartRange,
    CryptoChartInterval,
    PollSeconds,
    CryptoPollSeconds,
    CryptoAlertThreshold,
    CryptoAlertWindow,
    PopulationThresholdAlertsEnabled,
    PopulationAlertThreshold,
    TokenPriceAboveAlertsEnabled,
    TokenPriceAboveTarget,
    TokenPriceBelowAlertsEnabled,
    TokenPriceBelowTarget,
    ReleaseAlertsEnabled,
    AdvancedAlertCooldown,
    AdvancedAlertQuietHoursEnabled,
    AdvancedAlertQuietStartMinute,
    AdvancedAlertQuietEndMinute,
    AdvancedAlertMutes,
    MenuBarDisplayMode,
    WelcomeDismissed,
    AllTimePeak,
    AllTimePeakDate,
    LastAlertedPrice,
    LastAlertedPriceDate,
}

impl DefaultsKey {
    pub const ALL: [Self; 30] = [
        Self::AlertsEnabled,
        Self::PeakAlertsEnabled,
        Self::CryptoAlertsEnabled,
        Self::TokenChangeGainAlertsEnabled,
        Self::TokenChangeLossAlertsEnabled,
        Self::ChartInterval,
        Self::ChartRange,
        Self::CryptoChartInterval,
        Self::PollSeconds,
        Self::CryptoPollSeconds,
        Self::CryptoAlertThreshold,
        Self::CryptoAlertWindow,
        Self::PopulationThresholdAlertsEnabled,
        Self::PopulationAlertThreshold,
        Self::TokenPriceAboveAlertsEnabled,
        Self::TokenPriceAboveTarget,
        Self::TokenPriceBelowAlertsEnabled,
        Self::TokenPriceBelowTarget,
        Self::ReleaseAlertsEnabled,
        Self::AdvancedAlertCooldown,
        Self::AdvancedAlertQuietHoursEnabled,
        Self::AdvancedAlertQuietStartMinute,
        Self::AdvancedAlertQuietEndMinute,
        Self::AdvancedAlertMutes,
        Self::MenuBarDisplayMode,
        Self::WelcomeDismissed,
        Self::AllTimePeak,
        Self::AllTimePeakDate,
        Self::LastAlertedPrice,
        Self::LastAlertedPriceDate,
    ];

    pub const fn raw_name(self) -> &'static str {
        match self {
            Self::AlertsEnabled => "alertsEnabled",
            Self::PeakAlertsEnabled => "peakAlertsEnabled",
            Self::CryptoAlertsEnabled => "cryptoAlertsEnabled",
            Self::TokenChangeGainAlertsEnabled => "tokenChangeGainAlertsEnabled",
            Self::TokenChangeLossAlertsEnabled => "tokenChangeLossAlertsEnabled",
            Self::ChartInterval => "chartInterval",
            Self::ChartRange => "chartRange",
            Self::CryptoChartInterval => "cryptoChartInterval",
            Self::PollSeconds => "pollSeconds",
            Self::CryptoPollSeconds => "cryptoPollSeconds",
            Self::CryptoAlertThreshold => "cryptoAlertThreshold",
            Self::CryptoAlertWindow => "cryptoAlertWindow",
            Self::PopulationThresholdAlertsEnabled => "populationThresholdAlertsEnabled",
            Self::PopulationAlertThreshold => "populationAlertThreshold",
            Self::TokenPriceAboveAlertsEnabled => "tokenPriceAboveAlertsEnabled",
            Self::TokenPriceAboveTarget => "tokenPriceAboveTarget",
            Self::TokenPriceBelowAlertsEnabled => "tokenPriceBelowAlertsEnabled",
            Self::TokenPriceBelowTarget => "tokenPriceBelowTarget",
            Self::ReleaseAlertsEnabled => "releaseAlertsEnabled",
            Self::AdvancedAlertCooldown => "advancedAlertCooldown",
            Self::AdvancedAlertQuietHoursEnabled => "advancedAlertQuietHoursEnabled",
            Self::AdvancedAlertQuietStartMinute => "advancedAlertQuietStartMinute",
            Self::AdvancedAlertQuietEndMinute => "advancedAlertQuietEndMinute",
            Self::AdvancedAlertMutes => "advancedAlertMutes",
            Self::MenuBarDisplayMode => "menuBarDisplayMode",
            Self::WelcomeDismissed => "welcomeDismissed",
            Self::AllTimePeak => "allTimePeak",
            Self::AllTimePeakDate => "allTimePeakDate",
            Self::LastAlertedPrice => "lastAlertedPrice",
            Self::LastAlertedPriceDate => "lastAlertedPriceDate",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenChangeWindow {
    #[default]
    #[serde(rename = "oneHour")]
    OneHour,
    #[serde(rename = "sixHours")]
    SixHours,
    #[serde(rename = "twentyFourHours")]
    TwentyFourHours,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayMode {
    #[serde(rename = "players")]
    Players,
    #[default]
    #[serde(rename = "playersAndChange")]
    PlayersAndChange,
    #[serde(rename = "token")]
    Token,
    #[serde(rename = "full")]
    Full,
    #[serde(rename = "iconOnly")]
    IconOnly,
}

/// Typed, normalized settings snapshot. Launch-at-login is intentionally absent.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub alerts_enabled: bool,
    pub peak_alerts_enabled: bool,
    pub crypto_alerts_enabled: bool,
    pub token_change_gain_alerts_enabled: bool,
    pub token_change_loss_alerts_enabled: bool,
    pub chart_interval: i64,
    pub chart_range: i64,
    pub crypto_chart_interval: i64,
    pub poll_seconds: f64,
    pub crypto_poll_seconds: f64,
    pub crypto_alert_threshold: f64,
    pub crypto_alert_window: TokenChangeWindow,
    pub population_threshold_alerts_enabled: bool,
    pub population_alert_threshold: i64,
    pub token_price_above_alerts_enabled: bool,
    pub token_price_above_target: f64,
    pub token_price_below_alerts_enabled: bool,
    pub token_price_below_target: f64,
    pub release_alerts_enabled: bool,
    pub advanced_alert_cooldown: f64,
    pub advanced_alert_quiet_hours_enabled: bool,
    pub advanced_alert_quiet_start_minute: i32,
    pub advanced_alert_quiet_end_minute: i32,
    pub advanced_alert_mutes: BTreeMap<String, f64>,
    pub menu_bar_display_mode: DisplayMode,
    pub welcome_dismissed: bool,
    pub all_time_peak: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_time_peak_date: Option<f64>,
    pub last_alerted_price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_alerted_price_date: Option<f64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            alerts_enabled: true,
            peak_alerts_enabled: true,
            crypto_alerts_enabled: true,
            token_change_gain_alerts_enabled: true,
            token_change_loss_alerts_enabled: true,
            chart_interval: 300,
            chart_range: 21_600,
            crypto_chart_interval: 300,
            poll_seconds: poll::DEFAULT_PLAYER_SECONDS,
            crypto_poll_seconds: poll::DEFAULT_CRYPTO_SECONDS,
            crypto_alert_threshold: crypto_alert::DEFAULT_THRESHOLD_PERCENT,
            crypto_alert_window: TokenChangeWindow::OneHour,
            population_threshold_alerts_enabled: false,
            population_alert_threshold: advanced_alert::DEFAULT_POPULATION_THRESHOLD,
            token_price_above_alerts_enabled: false,
            token_price_above_target: advanced_alert::DEFAULT_PRICE_ABOVE_TARGET,
            token_price_below_alerts_enabled: false,
            token_price_below_target: advanced_alert::DEFAULT_PRICE_BELOW_TARGET,
            release_alerts_enabled: true,
            advanced_alert_cooldown: advanced_alert::default_cooldown(),
            advanced_alert_quiet_hours_enabled: false,
            advanced_alert_quiet_start_minute: advanced_alert::DEFAULT_QUIET_START_MINUTE as i32,
            advanced_alert_quiet_end_minute: advanced_alert::DEFAULT_QUIET_END_MINUTE as i32,
            advanced_alert_mutes: BTreeMap::new(),
            menu_bar_display_mode: DisplayMode::PlayersAndChange,
            welcome_dismissed: false,
            all_time_peak: 0,
            all_time_peak_date: None,
            last_alerted_price: 0.0,
            last_alerted_price_date: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsChange {
    pub key: DefaultsKey,
    pub settings: Settings,
}

pub struct SettingsStore {
    path: PathBuf,
    settings: Settings,
    subscribers: Vec<Sender<SettingsChange>>,
}

impl SettingsStore {
    pub fn open() -> io::Result<Self> {
        Self::open_at(Self::default_path()?)
    }
    pub fn open_at(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let raw = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Value::Object(Map::new()),
            Err(error) => return Err(error),
        };
        let settings = Settings::from_value(&raw);
        let store = Self {
            path,
            settings,
            subscribers: Vec::new(),
        };
        store.persist()?;
        Ok(store)
    }
    pub fn default_path() -> io::Result<PathBuf> {
        if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
            return Ok(PathBuf::from(path).join("woc-widget/settings.json"));
        }
        env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".config/woc-widget/settings.json"))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "neither XDG_CONFIG_HOME nor HOME is set",
                )
            })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn settings(&self) -> &Settings {
        &self.settings
    }
    pub fn subscribe(&mut self) -> Receiver<SettingsChange> {
        let (tx, rx) = mpsc::channel();
        self.subscribers.push(tx);
        rx
    }
    pub fn update(
        &mut self,
        key: DefaultsKey,
        mutate: impl FnOnce(&mut Settings),
    ) -> io::Result<bool> {
        let before = self.settings.clone();
        mutate(&mut self.settings);
        self.settings.normalize();
        if self.settings == before {
            return Ok(false);
        }
        if let Err(error) = self.persist() {
            self.settings = before;
            return Err(error);
        }
        let change = SettingsChange {
            key,
            settings: self.settings.clone(),
        };
        self.subscribers
            .retain(|subscriber| subscriber.send(change.clone()).is_ok());
        Ok(true)
    }

    /// Applies one Settings-panel edit through the same normalization and atomic persistence
    /// path as notification actions. Only Phase 13 user-editable frozen keys are accepted.
    pub fn update_value(&mut self, raw_key: &str, value: Value) -> Result<bool, String> {
        let key = editable_key(raw_key).ok_or_else(|| format!("unsupported setting: {raw_key}"))?;
        let mut candidate = self.settings.clone();
        apply_value(&mut candidate, key, value)?;
        self.update(key, |settings| *settings = candidate)
            .map_err(|error| error.to_string())
    }
    fn persist(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&self.settings).map_err(io::Error::other)?;
        {
            use std::io::Write;
            let mut file = fs::File::create(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        fs::rename(&temporary, &self.path)?;
        if let Some(parent) = self.path.parent() {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    }
}

fn editable_key(raw: &str) -> Option<DefaultsKey> {
    DefaultsKey::ALL.into_iter().find(|key| {
        key.raw_name() == raw
            && !matches!(
                key,
                DefaultsKey::ChartInterval
                    | DefaultsKey::ChartRange
                    | DefaultsKey::CryptoChartInterval
                    | DefaultsKey::AdvancedAlertMutes
                    | DefaultsKey::WelcomeDismissed
                    | DefaultsKey::AllTimePeak
                    | DefaultsKey::AllTimePeakDate
                    | DefaultsKey::LastAlertedPrice
                    | DefaultsKey::LastAlertedPriceDate
            )
    })
}

fn apply_value(settings: &mut Settings, key: DefaultsKey, value: Value) -> Result<(), String> {
    let boolean = || {
        value
            .as_bool()
            .ok_or_else(|| "expected a boolean".to_owned())
    };
    let number = || {
        value
            .as_f64()
            .ok_or_else(|| "expected a finite number".to_owned())
    };
    match key {
        DefaultsKey::AlertsEnabled => settings.alerts_enabled = boolean()?,
        DefaultsKey::PeakAlertsEnabled => settings.peak_alerts_enabled = boolean()?,
        DefaultsKey::CryptoAlertsEnabled => settings.crypto_alerts_enabled = boolean()?,
        DefaultsKey::TokenChangeGainAlertsEnabled => {
            settings.token_change_gain_alerts_enabled = boolean()?
        }
        DefaultsKey::TokenChangeLossAlertsEnabled => {
            settings.token_change_loss_alerts_enabled = boolean()?
        }
        DefaultsKey::PollSeconds => settings.poll_seconds = number()?,
        DefaultsKey::CryptoPollSeconds => settings.crypto_poll_seconds = number()?,
        DefaultsKey::CryptoAlertThreshold => settings.crypto_alert_threshold = number()?,
        DefaultsKey::CryptoAlertWindow => {
            settings.crypto_alert_window = serde_json::from_value(value)
                .map_err(|_| "unsupported crypto alert window".to_owned())?
        }
        DefaultsKey::PopulationThresholdAlertsEnabled => {
            settings.population_threshold_alerts_enabled = boolean()?
        }
        DefaultsKey::PopulationAlertThreshold => {
            settings.population_alert_threshold = value
                .as_i64()
                .ok_or_else(|| "expected an integer".to_owned())?
        }
        DefaultsKey::TokenPriceAboveAlertsEnabled => {
            settings.token_price_above_alerts_enabled = boolean()?
        }
        DefaultsKey::TokenPriceAboveTarget => settings.token_price_above_target = number()?,
        DefaultsKey::TokenPriceBelowAlertsEnabled => {
            settings.token_price_below_alerts_enabled = boolean()?
        }
        DefaultsKey::TokenPriceBelowTarget => settings.token_price_below_target = number()?,
        DefaultsKey::ReleaseAlertsEnabled => settings.release_alerts_enabled = boolean()?,
        DefaultsKey::AdvancedAlertCooldown => settings.advanced_alert_cooldown = number()?,
        DefaultsKey::AdvancedAlertQuietHoursEnabled => {
            settings.advanced_alert_quiet_hours_enabled = boolean()?
        }
        DefaultsKey::AdvancedAlertQuietStartMinute => {
            settings.advanced_alert_quiet_start_minute = value
                .as_i64()
                .and_then(|value| i32::try_from(value).ok())
                .ok_or_else(|| "expected a minute integer".to_owned())?
        }
        DefaultsKey::AdvancedAlertQuietEndMinute => {
            settings.advanced_alert_quiet_end_minute = value
                .as_i64()
                .and_then(|value| i32::try_from(value).ok())
                .ok_or_else(|| "expected a minute integer".to_owned())?
        }
        DefaultsKey::MenuBarDisplayMode => {
            settings.menu_bar_display_mode = serde_json::from_value(value)
                .map_err(|_| "unsupported menu bar display mode".to_owned())?
        }
        _ => return Err(format!("unsupported setting: {}", key.raw_name())),
    }
    Ok(())
}

impl Settings {
    fn from_value(value: &Value) -> Self {
        let d = Self::default();
        let object = value.as_object();
        let get = |key: DefaultsKey| object.and_then(|o| o.get(key.raw_name()));
        let boolean = |key, fallback| get(key).and_then(Value::as_bool).unwrap_or(fallback);
        let integer = |key, fallback| get(key).and_then(Value::as_i64).unwrap_or(fallback);
        let number = |key, fallback| {
            get(key)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .unwrap_or(fallback)
        };
        let enum_value = |key, fallback: Value| get(key).cloned().unwrap_or(fallback);
        let mut result = Self {
            alerts_enabled: boolean(DefaultsKey::AlertsEnabled, d.alerts_enabled),
            peak_alerts_enabled: boolean(DefaultsKey::PeakAlertsEnabled, d.peak_alerts_enabled),
            crypto_alerts_enabled: boolean(
                DefaultsKey::CryptoAlertsEnabled,
                d.crypto_alerts_enabled,
            ),
            token_change_gain_alerts_enabled: boolean(
                DefaultsKey::TokenChangeGainAlertsEnabled,
                d.token_change_gain_alerts_enabled,
            ),
            token_change_loss_alerts_enabled: boolean(
                DefaultsKey::TokenChangeLossAlertsEnabled,
                d.token_change_loss_alerts_enabled,
            ),
            chart_interval: integer(DefaultsKey::ChartInterval, d.chart_interval),
            chart_range: integer(DefaultsKey::ChartRange, d.chart_range),
            crypto_chart_interval: integer(
                DefaultsKey::CryptoChartInterval,
                d.crypto_chart_interval,
            ),
            poll_seconds: number(DefaultsKey::PollSeconds, d.poll_seconds),
            crypto_poll_seconds: number(DefaultsKey::CryptoPollSeconds, d.crypto_poll_seconds),
            crypto_alert_threshold: number(
                DefaultsKey::CryptoAlertThreshold,
                d.crypto_alert_threshold,
            ),
            crypto_alert_window: serde_json::from_value(enum_value(
                DefaultsKey::CryptoAlertWindow,
                Value::String("oneHour".into()),
            ))
            .unwrap_or_default(),
            population_threshold_alerts_enabled: boolean(
                DefaultsKey::PopulationThresholdAlertsEnabled,
                d.population_threshold_alerts_enabled,
            ),
            population_alert_threshold: integer(
                DefaultsKey::PopulationAlertThreshold,
                d.population_alert_threshold,
            ),
            token_price_above_alerts_enabled: boolean(
                DefaultsKey::TokenPriceAboveAlertsEnabled,
                d.token_price_above_alerts_enabled,
            ),
            token_price_above_target: number(
                DefaultsKey::TokenPriceAboveTarget,
                d.token_price_above_target,
            ),
            token_price_below_alerts_enabled: boolean(
                DefaultsKey::TokenPriceBelowAlertsEnabled,
                d.token_price_below_alerts_enabled,
            ),
            token_price_below_target: number(
                DefaultsKey::TokenPriceBelowTarget,
                d.token_price_below_target,
            ),
            release_alerts_enabled: boolean(
                DefaultsKey::ReleaseAlertsEnabled,
                d.release_alerts_enabled,
            ),
            advanced_alert_cooldown: number(
                DefaultsKey::AdvancedAlertCooldown,
                d.advanced_alert_cooldown,
            ),
            advanced_alert_quiet_hours_enabled: boolean(
                DefaultsKey::AdvancedAlertQuietHoursEnabled,
                d.advanced_alert_quiet_hours_enabled,
            ),
            advanced_alert_quiet_start_minute: integer(
                DefaultsKey::AdvancedAlertQuietStartMinute,
                d.advanced_alert_quiet_start_minute as i64,
            ) as i32,
            advanced_alert_quiet_end_minute: integer(
                DefaultsKey::AdvancedAlertQuietEndMinute,
                d.advanced_alert_quiet_end_minute as i64,
            ) as i32,
            advanced_alert_mutes: get(DefaultsKey::AdvancedAlertMutes)
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| {
                            v.as_f64().filter(|v| v.is_finite()).map(|v| (k.clone(), v))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            menu_bar_display_mode: serde_json::from_value(enum_value(
                DefaultsKey::MenuBarDisplayMode,
                Value::String("playersAndChange".into()),
            ))
            .unwrap_or_default(),
            welcome_dismissed: boolean(DefaultsKey::WelcomeDismissed, d.welcome_dismissed),
            all_time_peak: integer(DefaultsKey::AllTimePeak, 0),
            all_time_peak_date: get(DefaultsKey::AllTimePeakDate)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite()),
            last_alerted_price: number(DefaultsKey::LastAlertedPrice, 0.0),
            last_alerted_price_date: get(DefaultsKey::LastAlertedPriceDate)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite()),
        };
        result.normalize();
        result
    }
    fn normalize(&mut self) {
        const CHART_INTERVALS: [i64; 5] = [60, 300, 900, 1800, 3600];
        const CHART_RANGES: [i64; 4] = [3600, 21600, 86400, 604800];
        const CANDLES: [i64; 5] = [60, 300, 900, 3600, 14400];
        if !CHART_INTERVALS.contains(&self.chart_interval) {
            self.chart_interval = 300;
        }
        if !CHART_RANGES.contains(&self.chart_range) {
            self.chart_range = 21_600;
        }
        if !CANDLES.contains(&self.crypto_chart_interval) {
            self.crypto_chart_interval = 300;
        }
        self.poll_seconds = PollInterval::normalize(
            self.poll_seconds,
            &PollInterval::player_options(),
            PollInterval::OneMinute,
        );
        self.crypto_poll_seconds = PollInterval::normalize(
            self.crypto_poll_seconds,
            &PollInterval::crypto_options(),
            PollInterval::OneMinute,
        );
        self.crypto_alert_threshold = if self.crypto_alert_threshold.is_finite() {
            self.crypto_alert_threshold
                .clamp(
                    *crypto_alert::SLIDER_RANGE.start(),
                    *crypto_alert::SLIDER_RANGE.end(),
                )
                .round()
        } else {
            crypto_alert::DEFAULT_THRESHOLD_PERCENT
        };
        self.population_alert_threshold =
            advanced_alert::normalize_population(self.population_alert_threshold);
        self.token_price_above_target = advanced_alert::normalize_price_target(
            self.token_price_above_target,
            advanced_alert::DEFAULT_PRICE_ABOVE_TARGET,
        );
        self.token_price_below_target = advanced_alert::normalize_price_target(
            self.token_price_below_target,
            advanced_alert::DEFAULT_PRICE_BELOW_TARGET,
        );
        self.advanced_alert_cooldown =
            advanced_alert::normalize_cooldown(self.advanced_alert_cooldown);
        self.advanced_alert_quiet_start_minute =
            minute_of_day(self.advanced_alert_quiet_start_minute);
        self.advanced_alert_quiet_end_minute = minute_of_day(self.advanced_alert_quiet_end_minute);
        self.all_time_peak = self.all_time_peak.max(0);
        if self.all_time_peak == 0 {
            self.all_time_peak_date = None;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64());
        self.advanced_alert_mutes.retain(|key, until| {
            matches!(
                key.as_str(),
                "realm-status"
                    | "local-record"
                    | "population-threshold"
                    | "token-price-above"
                    | "token-price-below"
                    | "token-change-gain"
                    | "token-change-loss"
                    | "game-release"
            ) && until.is_finite()
                && *until > now
        });
        let baseline_fresh = self.last_alerted_price.is_finite()
            && self.last_alerted_price > 0.0
            && self.last_alerted_price_date.is_some_and(|date| {
                date.is_finite()
                    && now - date >= 0.0
                    && now - date <= crypto_alert::MAXIMUM_BASELINE_AGE
            });
        if !baseline_fresh {
            self.last_alerted_price = 0.0;
            self.last_alerted_price_date = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(name: &str) -> PathBuf {
        env::temp_dir().join(format!("woc-settings-{}-{name}.json", std::process::id()))
    }
    #[test]
    fn frozen_keys_are_complete_and_launch_login_is_absent() {
        assert_eq!(DefaultsKey::ALL.len(), 30);
        let names: Vec<_> = DefaultsKey::ALL.iter().map(|k| k.raw_name()).collect();
        assert!(!names.contains(&"launchAtLogin"));
        assert_eq!(
            names,
            [
                "alertsEnabled",
                "peakAlertsEnabled",
                "cryptoAlertsEnabled",
                "tokenChangeGainAlertsEnabled",
                "tokenChangeLossAlertsEnabled",
                "chartInterval",
                "chartRange",
                "cryptoChartInterval",
                "pollSeconds",
                "cryptoPollSeconds",
                "cryptoAlertThreshold",
                "cryptoAlertWindow",
                "populationThresholdAlertsEnabled",
                "populationAlertThreshold",
                "tokenPriceAboveAlertsEnabled",
                "tokenPriceAboveTarget",
                "tokenPriceBelowAlertsEnabled",
                "tokenPriceBelowTarget",
                "releaseAlertsEnabled",
                "advancedAlertCooldown",
                "advancedAlertQuietHoursEnabled",
                "advancedAlertQuietStartMinute",
                "advancedAlertQuietEndMinute",
                "advancedAlertMutes",
                "menuBarDisplayMode",
                "welcomeDismissed",
                "allTimePeak",
                "allTimePeakDate",
                "lastAlertedPrice",
                "lastAlertedPriceDate",
            ]
        );
    }
    #[test]
    fn registers_defaults_at_xdg_shape() {
        let p = path("defaults");
        let store = SettingsStore::open_at(&p).unwrap();
        assert_eq!(store.settings(), &Settings::default());
        let value: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        assert_eq!(value["pollSeconds"], 60.0);
        assert!(value.get("launchAtLogin").is_none());
        let _ = fs::remove_file(p);
    }
    #[test]
    fn corrupt_values_normalize_and_repairs_persist() {
        let p = path("repair");
        fs::write(&p,br#"{"chartInterval":7,"chartRange":7,"cryptoChartInterval":7,"pollSeconds":2,"cryptoPollSeconds":121,"cryptoAlertThreshold":99,"populationAlertThreshold":-3,"tokenPriceAboveTarget":-1,"tokenPriceBelowTarget":99999999,"advancedAlertCooldown":1000,"advancedAlertQuietStartMinute":-60,"advancedAlertQuietEndMinute":1441,"menuBarDisplayMode":"bad","cryptoAlertWindow":"bad","allTimePeak":-4}"#).unwrap();
        let s = SettingsStore::open_at(&p).unwrap();
        let x = s.settings();
        assert_eq!(
            (x.chart_interval, x.chart_range, x.crypto_chart_interval),
            (300, 21600, 300)
        );
        assert_eq!((x.poll_seconds, x.crypto_poll_seconds), (10.0, 60.0));
        assert_eq!(x.crypto_alert_threshold, 50.0);
        assert_eq!(x.population_alert_threshold, 1);
        assert_eq!(x.token_price_above_target, 0.001);
        assert_eq!(x.token_price_below_target, 1_000_000.0);
        assert_eq!(x.advanced_alert_cooldown, 900.0);
        assert_eq!(
            (
                x.advanced_alert_quiet_start_minute,
                x.advanced_alert_quiet_end_minute
            ),
            (1380, 1)
        );
        assert_eq!(x.menu_bar_display_mode, DisplayMode::PlayersAndChange);
        let persisted: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        assert_eq!(persisted["pollSeconds"], 10.0);
        assert_eq!(persisted["menuBarDisplayMode"], "playersAndChange");
        let _ = fs::remove_file(p);
    }

    #[test]
    fn linux_icon_only_display_mode_round_trips() {
        let value = serde_json::json!({ "menuBarDisplayMode": "iconOnly" });
        let settings = Settings::from_value(&value);
        assert_eq!(settings.menu_bar_display_mode, DisplayMode::IconOnly);
        assert_eq!(
            serde_json::to_value(settings).unwrap()["menuBarDisplayMode"],
            "iconOnly"
        );
    }
    #[test]
    fn panel_updates_normalize_and_reject_internal_or_wrong_typed_keys() {
        let p = path("panel-update");
        let mut store = SettingsStore::open_at(&p).unwrap();
        store
            .update_value("populationAlertThreshold", serde_json::json!(10_004))
            .unwrap();
        assert_eq!(store.settings().population_alert_threshold, 10_000);
        store
            .update_value("menuBarDisplayMode", serde_json::json!("iconOnly"))
            .unwrap();
        assert_eq!(
            store.settings().menu_bar_display_mode,
            DisplayMode::IconOnly
        );
        assert!(store
            .update_value("allTimePeak", serde_json::json!(42))
            .is_err());
        assert!(store
            .update_value("alertsEnabled", serde_json::json!("yes"))
            .is_err());
        let persisted: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        assert_eq!(persisted["populationAlertThreshold"], 10_000);
        let _ = fs::remove_file(p);
    }
    #[test]
    fn nearest_poll_option_ties_prefer_longer() {
        let p = path("tie");
        fs::write(&p, br#"{"pollSeconds":45,"cryptoPollSeconds":180}"#).unwrap();
        let s = SettingsStore::open_at(&p).unwrap();
        assert_eq!(s.settings().poll_seconds, 60.0);
        assert_eq!(s.settings().crypto_poll_seconds, 300.0);
        let _ = fs::remove_file(p);
    }
    #[test]
    fn update_persists_normalizes_and_notifies() {
        let p = path("notify");
        let mut s = SettingsStore::open_at(&p).unwrap();
        let rx = s.subscribe();
        assert!(s
            .update(DefaultsKey::PopulationAlertThreshold, |x| x
                .population_alert_threshold =
                50_000)
            .unwrap());
        let event = rx.recv().unwrap();
        assert_eq!(event.key, DefaultsKey::PopulationAlertThreshold);
        assert_eq!(event.settings.population_alert_threshold, 10_000);
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&p).unwrap()).unwrap()
                ["populationAlertThreshold"],
            10_000
        );
        let _ = fs::remove_file(p);
    }

    #[test]
    fn failed_update_rolls_back_memory_and_does_not_notify() {
        let p = path("rollback");
        let mut store = SettingsStore::open_at(&p).unwrap();
        let changes = store.subscribe();
        let before = store.settings().clone();
        let blocked_temporary = p.with_extension("json.tmp");
        fs::create_dir_all(&blocked_temporary).unwrap();

        assert!(store
            .update(DefaultsKey::PollSeconds, |settings| {
                settings.poll_seconds = 10.0;
            })
            .is_err());
        assert_eq!(store.settings(), &before);
        assert!(changes.try_recv().is_err());
        let disk: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        assert_eq!(disk["pollSeconds"], before.poll_seconds);

        let _ = fs::remove_dir(blocked_temporary);
        let _ = fs::remove_file(p);
    }
}
