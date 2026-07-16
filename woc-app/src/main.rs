//! Tauri shell and always-running application-service composition.

use std::{
    collections::HashMap,
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::{Arc, Mutex},
};

use chrono::{Offset, TimeZone};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, RunEvent};
use tauri_plugin_dialog::DialogExt;
use woc_app::{
    app_state::AppState,
    autostart::Autostart,
    community::{CommunityFeed, CommunityFeedState, FeedPhase},
    cosmic_panel::{self, CosmicConfig},
    lifecycle::LifecycleFlush,
    notifications::{AppStateNotificationActions, NotificationDispatcher, NotifyRustBackend},
    settings::{DefaultsKey, Settings, TokenChangeWindow},
    strings, tray,
};
use wockit::alerts::core::{
    evaluate_crypto, evaluate_debounced_status, evaluate_peak, AlertDecision, AlertSettings,
    StatusAlertObservation, StatusAlertState,
};
use wockit::alerts::policy::{
    evaluate_with_mutes, population_hysteresis, price_hysteresis, rolling_hysteresis, rule_catalog,
    suppression_reason, AdvancedAlertRuleId, ChangeDirection, ChangePolicy, ChangeWindow, Decision,
    Observation, Payload, PolicySet, PolicyState, PopulationPolicy, PricePolicy, QuietHours,
    ReleasePayload, RuleConfiguration, RuleMutes, ThresholdDirection,
};
use wockit::history::{
    export::{export_data, HistoryExportFormat},
    HistorySample,
};
use wockit::{
    analytics::{segment_history, window_coverage, HistoryAnalytics},
    models::{
        CandleInterval, ChartInterval, ChartRange, CryptoMarketTimeframe, GameRealm, GameRelease,
        LifetimeLeaderboard, LifetimeLeaderboardEntry, RealmDirectory, ReleaseFeed,
    },
};

type LiveNotifications = Arc<NotificationDispatcher<NotifyRustBackend>>;
type DeliveryClocks = Arc<Mutex<HashMap<AdvancedAlertRuleId, chrono::DateTime<chrono::Utc>>>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardSnapshot {
    realm: String,
    players_online: Option<i64>,
    realm_state: &'static str,
    status_feed_state: &'static str,
    status_last_success_ms: Option<i64>,
    status_refreshing: bool,
    price: Option<String>,
    change_24h: Option<f64>,
    quote_feed_state: &'static str,
    quote_last_success_ms: Option<i64>,
    quote_refreshing: bool,
    candle_last_success_ms: Option<i64>,
    candle_feed_state: &'static str,
    candle_refreshing: bool,
    community_last_success_ms: Option<i64>,
    community_feed_state: &'static str,
    community_refreshing: bool,
    snapshot_time_ms: i64,
    chart_range_seconds: i64,
    chart_interval_seconds: i64,
    chart_interval_label: String,
    chart_points: Vec<ChartPoint>,
    chart_y_domain: Option<ChartYDomain>,
    window_coverage_percent: i64,
    short_change: Option<i64>,
    today_high: Option<i64>,
    local_record: i64,
    range_average: Option<i64>,
    rhythm: RhythmSnapshot,
    welcome_dismissed: bool,
    population_alert_threshold: i64,
    market_quote: Option<MarketQuoteSnapshot>,
    candles: Vec<CandleSnapshot>,
    selected_candle_interval: String,
    loaded_candle_interval: Option<String>,
    crypto_alert_threshold: f64,
    crypto_alert_window: &'static str,
    community: CommunityUiSnapshot,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CommunityUiSnapshot {
    project_stats: CommunityFeedUiState<ProjectStatsUi>,
    releases: CommunityFeedUiState<Vec<GameRelease>>,
    leaderboard: CommunityFeedUiState<Vec<LifetimeLeaderboardEntry>>,
    realms: CommunityFeedUiState<Vec<GameRealm>>,
    current_realm: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectStatsUi {
    accounts_created: Option<i64>,
    players_online: Option<i64>,
    realm: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CommunityFeedUiState<T> {
    value: Option<T>,
    phase: &'static str,
    last_success_ms: Option<i64>,
}

fn community_feed_ui<T, U>(
    state: CommunityFeedState<T>,
    map: impl FnOnce(T) -> U,
) -> CommunityFeedUiState<U> {
    CommunityFeedUiState {
        value: state.value.map(map),
        phase: match state.phase {
            FeedPhase::Idle => "idle",
            FeedPhase::Loading => "loading",
            FeedPhase::Loaded => "live",
            FeedPhase::Cached => "cached",
            FeedPhase::Failed => "unavailable",
        },
        last_success_ms: system_time_as_unix_ms(state.last_success),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketQuoteSnapshot {
    price: String,
    change_24h: f64,
    liquidity_usd: Option<f64>,
    market_cap_usd: Option<f64>,
    fully_diluted_valuation_usd: Option<f64>,
    pair_url: String,
    windows: HashMap<&'static str, MarketWindowSnapshot>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketWindowSnapshot {
    change_percent: Option<f64>,
    buys: Option<i64>,
    sells: Option<i64>,
    volume_usd: Option<f64>,
}

#[derive(Serialize)]
struct CandleSnapshot {
    time: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChartPoint {
    time: i64,
    value: Option<i64>,
    isolated: bool,
}

#[derive(Serialize)]
struct ChartYDomain {
    min: i64,
    max: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RhythmSnapshot {
    sample_count: usize,
    coverage_percent: i64,
    current_percentile: Option<i64>,
}

fn chart_range(seconds: i64) -> ChartRange {
    ChartRange::ALL_CASES
        .into_iter()
        .find(|range| range.seconds() == seconds)
        .unwrap_or(ChartRange::SixHours)
}

fn market_timeframe_key(timeframe: CryptoMarketTimeframe) -> &'static str {
    match timeframe {
        CryptoMarketTimeframe::FiveMinutes => "5m",
        CryptoMarketTimeframe::OneHour => "1h",
        CryptoMarketTimeframe::SixHours => "6h",
        CryptoMarketTimeframe::TwentyFourHours => "24h",
    }
}

fn crypto_alert_window_name(window: TokenChangeWindow) -> &'static str {
    match window {
        TokenChangeWindow::OneHour => "1h",
        TokenChangeWindow::SixHours => "6h",
        TokenChangeWindow::TwentyFourHours => "24h",
    }
}

fn feed_state_name(state: woc_app::poller::DataFeedState) -> &'static str {
    use woc_app::poller::DataFeedState;
    match state {
        DataFeedState::Idle => "idle",
        DataFeedState::Loading => "loading",
        DataFeedState::Live => "live",
        DataFeedState::Cached => "cached",
        DataFeedState::Unavailable => "unavailable",
    }
}

fn instant_as_unix_ms(value: Option<tokio::time::Instant>) -> Option<i64> {
    let age = tokio::time::Instant::now().checked_duration_since(value?)?;
    let time = std::time::SystemTime::now().checked_sub(age)?;
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

fn system_time_as_unix_ms(value: Option<std::time::SystemTime>) -> Option<i64> {
    value?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

#[tauri::command]
async fn dashboard_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DashboardSnapshot, String> {
    let poller = state.poller.snapshot().await;
    let status = poller.status.value.as_ref();
    let quote = poller.quote.value.as_ref();
    let community_phase = state.community.phase();
    let community = state.community.snapshot();
    let settings = state.settings.lock().unwrap().settings().clone();
    let now = chrono::Utc::now();
    let history = state
        .history
        .snapshot()
        .map_err(|error| error.to_string())?;
    let range = chart_range(settings.chart_range);
    let interval = ChartInterval::automatic(range);
    let analytics = HistoryAnalytics::new(&history, now);
    let series = analytics.series(range, interval);
    let current_count = status.map(|value| value.players_online);
    let timezone = local_timezone();
    let local_offset = timezone.offset_from_utc_datetime(&now.naive_utc()).fix();
    let mut rhythm = analytics.realm_rhythm(
        range,
        current_count.unwrap_or_default(),
        settings.poll_seconds,
    );
    if current_count.is_none() {
        rhythm.current_percentile = None;
    }
    let segmented = segment_history(&series, interval.seconds() as f64, settings.poll_seconds);
    let mut chart_points = Vec::with_capacity(segmented.len() * 2);
    let mut previous_segment = None;
    let mut previous_time = None;
    for point in segmented {
        if previous_segment.is_some_and(|segment| segment != point.segment) {
            chart_points.push(ChartPoint {
                time: previous_time.unwrap_or(point.sample.date.timestamp()) + 1,
                value: None,
                isolated: false,
            });
            chart_points.push(ChartPoint {
                time: point.sample.date.timestamp() - 1,
                value: None,
                isolated: false,
            });
        }
        previous_segment = Some(point.segment);
        previous_time = Some(point.sample.date.timestamp());
        chart_points.push(ChartPoint {
            time: point.sample.date.timestamp(),
            value: Some(point.sample.count),
            isolated: point.is_isolated,
        });
    }
    let chart_y_domain = series
        .iter()
        .map(|sample| sample.count)
        .min()
        .zip(series.iter().map(|sample| sample.count).max())
        .map(|(minimum, maximum)| ChartYDomain {
            min: minimum.saturating_sub(3).max(0),
            max: maximum.saturating_add(3),
        });
    let realm_state = match poller.realm_availability {
        woc_app::poller::RealmAvailability::Loading => "loading",
        woc_app::poller::RealmAvailability::Healthy => "healthy",
        woc_app::poller::RealmAvailability::ServerReportedDown => "offline",
        woc_app::poller::RealmAvailability::Unreachable(_) => "unreachable",
    };
    let market_quote = quote.map(|quote| MarketQuoteSnapshot {
        price: quote.price.clone(),
        change_24h: quote.change24h,
        liquidity_usd: quote.liquidity_usd,
        market_cap_usd: quote.market_cap_usd,
        fully_diluted_valuation_usd: quote.fdv_usd,
        pair_url: quote
            .pair_url
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                format!(
                    "https://dexscreener.com/{}/{}",
                    wockit::config::api::DEX_CHAIN,
                    wockit::config::api::DEX_PAIR_ADDRESS
                )
            }),
        windows: CryptoMarketTimeframe::ALL_CASES
            .into_iter()
            .filter_map(|timeframe| {
                quote.metrics(timeframe).map(|window| {
                    (
                        market_timeframe_key(timeframe),
                        MarketWindowSnapshot {
                            change_percent: window.change_percent,
                            buys: window.buys,
                            sells: window.sells,
                            volume_usd: window.volume_usd,
                        },
                    )
                })
            })
            .collect(),
    });
    let candles = poller
        .candles
        .value
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|candle| CandleSnapshot {
            time: candle.date.timestamp(),
            open: candle.open,
            high: candle.high,
            low: candle.low,
            close: candle.close,
            volume: candle.volume,
        })
        .collect();
    Ok(DashboardSnapshot {
        realm: status
            .and_then(|value| value.realm.clone())
            .unwrap_or_else(|| "Claudemoon".into()),
        players_online: status.map(|value| value.players_online),
        realm_state,
        status_feed_state: feed_state_name(poller.status.state),
        status_last_success_ms: instant_as_unix_ms(poller.status.last_success),
        status_refreshing: poller.status.is_refreshing,
        price: quote.map(|value| value.price.clone()),
        change_24h: quote.map(|value| value.change24h),
        quote_feed_state: feed_state_name(poller.quote.state),
        quote_last_success_ms: instant_as_unix_ms(poller.quote.last_success),
        quote_refreshing: poller.quote.is_refreshing,
        candle_last_success_ms: instant_as_unix_ms(poller.candles.last_success),
        candle_feed_state: feed_state_name(poller.candles.state),
        candle_refreshing: poller.candles.is_refreshing,
        community_last_success_ms: system_time_as_unix_ms(state.community.last_success()),
        community_feed_state: match community_phase {
            woc_app::community::CommunityPhase::Idle => "idle",
            woc_app::community::CommunityPhase::Loading => "loading",
            woc_app::community::CommunityPhase::Loaded => "live",
            woc_app::community::CommunityPhase::Partial => "cached",
            woc_app::community::CommunityPhase::Failed => "unavailable",
        },
        community_refreshing: community_phase == woc_app::community::CommunityPhase::Loading,
        snapshot_time_ms: system_time_as_unix_ms(Some(std::time::SystemTime::now())).unwrap_or(0),
        chart_range_seconds: range.seconds(),
        chart_interval_seconds: interval.seconds(),
        chart_interval_label: interval.label(),
        chart_points,
        chart_y_domain,
        window_coverage_percent: (window_coverage(&series, now, range, interval) * 100.0).round()
            as i64,
        short_change: status
            .and_then(|value| analytics.short_change(value.players_online, settings.poll_seconds)),
        today_high: current_count.map(|count| analytics.today_high(count, local_offset)),
        local_record: settings.all_time_peak,
        range_average: HistoryAnalytics::average(&series),
        rhythm: RhythmSnapshot {
            sample_count: rhythm.sample_count,
            coverage_percent: (rhythm.coverage_fraction * 100.0).round() as i64,
            current_percentile: rhythm.current_percentile,
        },
        welcome_dismissed: settings.welcome_dismissed,
        population_alert_threshold: settings.population_alert_threshold,
        market_quote,
        candles,
        selected_candle_interval: poller.selected_candle_interval.label(),
        loaded_candle_interval: poller.loaded_candle_interval.map(CandleInterval::label),
        crypto_alert_threshold: settings.crypto_alert_threshold,
        crypto_alert_window: crypto_alert_window_name(settings.crypto_alert_window),
        community: CommunityUiSnapshot {
            current_realm: community
                .realms
                .value
                .as_ref()
                .and_then(|value| value.current_realm.clone()),
            project_stats: community_feed_ui(community.project_stats, |value| ProjectStatsUi {
                accounts_created: value.accounts_created,
                players_online: value.players_online,
                realm: value.realm,
            }),
            releases: community_feed_ui(community.releases, |value: ReleaseFeed| value.releases),
            leaderboard: community_feed_ui(community.leaderboard, |value: LifetimeLeaderboard| {
                value.leaders
            }),
            realms: community_feed_ui(community.realms, |value: RealmDirectory| value.realms),
        },
    })
}

#[tauri::command]
fn set_chart_range(seconds: i64, state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    if !ChartRange::ALL_CASES
        .iter()
        .any(|range| range.seconds() == seconds)
    {
        return Err(format!("unsupported chart range: {seconds}"));
    }
    state
        .settings
        .lock()
        .unwrap()
        .update(DefaultsKey::ChartRange, |settings| {
            settings.chart_range = seconds;
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
async fn set_candle_interval(
    seconds: i64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let interval = CandleInterval::ALL_CASES
        .into_iter()
        .find(|interval| interval.seconds() == seconds)
        .ok_or_else(|| format!("unsupported candle interval: {seconds}"))?;
    state
        .settings
        .lock()
        .unwrap()
        .update(DefaultsKey::CryptoChartInterval, |settings| {
            settings.crypto_chart_interval = seconds;
        })
        .map_err(|error| error.to_string())?;
    state.poller.set_candle_interval(interval).await;
    Ok(())
}

#[tauri::command]
fn dismiss_welcome(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    state
        .settings
        .lock()
        .unwrap()
        .update(DefaultsKey::WelcomeDismissed, |settings| {
            settings.welcome_dismissed = true;
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
async fn refresh_status(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    state.poller.refresh_status().await;
    Ok(())
}

#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    if !is_allowed_external_url(&url) {
        return Err("unsupported external URL".into());
    }
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn is_allowed_external_url(url: &str) -> bool {
    const ALLOWED: [&str; 3] = [
        "https://worldofclaudecraft.com/",
        "https://worldofclaudecraft.com/wiki/",
        "https://worldofclaudecraft.com/#highscores",
    ];
    let gecko_terminal = format!(
        "https://www.geckoterminal.com/{}/pools/{}",
        wockit::config::api::GECKO_NETWORK,
        wockit::config::api::GECKO_POOL
    );
    let is_market = wockit::config::api::validated_market_url(Some(url)).is_some();
    let is_community = url == "https://discord.com/invite/GjhnUsBtw"
        || url == "https://github.com/levy-street/world-of-claudecraft"
        || url.starts_with("https://github.com/levy-street/world-of-claudecraft/releases/");
    let is_about = matches!(
        url,
        "https://github.com/FernandoX7/woc-widget-linux"
            | "https://github.com/FernandoX7/woc-widget-linux/issues"
            | "https://github.com/FernandoX7/woc-widget-linux/blob/main/PRIVACY.md"
            | "https://github.com/FernandoX7/woc-widget-linux/blob/main/LICENSE"
    );
    ALLOWED.contains(&url) || is_market || url == gecko_terminal || is_community || is_about
}

#[tauri::command]
async fn refresh_page(page: String, state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    match page.as_str() {
        "overview" => {
            tokio::join!(state.poller.refresh_status(), state.poller.refresh_quote());
        }
        "market" => {
            tokio::join!(state.poller.refresh_quote(), state.poller.refresh_candles());
        }
        "community" => {
            state.community.refresh().await;
        }
        _ => return Err(format!("unknown dashboard page: {page}")),
    }
    Ok(())
}

#[tauri::command]
async fn refresh_community_if_needed(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    state.community.refresh_if_needed().await;
    Ok(())
}

#[tauri::command]
async fn refresh_community_feed(
    feed: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let feed = match feed.as_str() {
        "projectStats" => CommunityFeed::ProjectStats,
        "releases" => CommunityFeed::Releases,
        "leaderboard" => CommunityFeed::Leaderboard,
        "realms" => CommunityFeed::Realms,
        _ => return Err(format!("unknown community feed: {feed}")),
    };
    state.community.refresh_feed(feed).await;
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

fn deliver_if_allowed<T>(
    settings: &Settings,
    rule_id: &str,
    clocks: &DeliveryClocks,
    deliver: impl FnOnce() -> Result<T, String>,
) -> bool {
    let now = chrono::Utc::now();
    let enabled = match rule_id {
        rule_catalog::REALM_STATUS => settings.alerts_enabled,
        rule_catalog::LOCAL_RECORD => settings.peak_alerts_enabled,
        rule_catalog::POPULATION => settings.population_threshold_alerts_enabled,
        rule_catalog::TOKEN_PRICE_ABOVE => settings.token_price_above_alerts_enabled,
        rule_catalog::TOKEN_PRICE_BELOW => settings.token_price_below_alerts_enabled,
        rule_catalog::TOKEN_CHANGE_GAIN => settings.token_change_gain_alerts_enabled,
        rule_catalog::TOKEN_CHANGE_LOSS => settings.token_change_loss_alerts_enabled,
        rule_catalog::RELEASE => settings.release_alerts_enabled,
        _ => false,
    };
    let persisted = settings
        .advanced_alert_mutes
        .iter()
        .map(|(key, value)| (key.clone(), *value))
        .collect();
    let id = AdvancedAlertRuleId::new(rule_id);
    let mut mutes = RuleMutes::from_unix_seconds(&persisted, now);
    let rule = RuleConfiguration {
        id: id.clone(),
        enabled: enabled && !mutes.is_muted(&id, now),
        cooldown_seconds: settings.advanced_alert_cooldown,
    };
    let quiet = QuietHours::new(
        settings.advanced_alert_quiet_hours_enabled,
        settings.advanced_alert_quiet_start_minute,
        settings.advanced_alert_quiet_end_minute,
        local_timezone(),
    );
    let mut clocks = clocks.lock().unwrap();
    if suppression_reason(&rule, now, Some(&quiet), clocks.get(&id).copied()).is_some() {
        return false;
    }
    match deliver() {
        Ok(_) => {
            clocks.insert(id, now);
            true
        }
        Err(error) => {
            eprintln!("failed to dispatch {rule_id} notification: {error}");
            false
        }
    }
}

fn legacy_rule_id(decision: &AlertDecision) -> &'static str {
    match decision {
        AlertDecision::StatusUp { .. } | AlertDecision::StatusDown { .. } => {
            rule_catalog::REALM_STATUS
        }
        AlertDecision::Peak { .. } => rule_catalog::LOCAL_RECORD,
        AlertDecision::CryptoPump { .. } => rule_catalog::TOKEN_CHANGE_GAIN,
        AlertDecision::CryptoDump { .. } => rule_catalog::TOKEN_CHANGE_LOSS,
    }
}

fn configured_rule(id: &str, enabled: bool, settings: &Settings) -> RuleConfiguration {
    RuleConfiguration {
        id: AdvancedAlertRuleId::new(id),
        enabled,
        cooldown_seconds: settings.advanced_alert_cooldown,
    }
}

fn local_timezone() -> chrono_tz::Tz {
    std::env::var("TZ")
        .ok()
        .and_then(|value| value.parse().ok())
        .or_else(|| {
            std::fs::read_to_string("/etc/timezone")
                .ok()
                .and_then(|value| value.trim().parse().ok())
        })
        .unwrap_or(chrono_tz::UTC)
}

fn alert_policies(settings: &Settings) -> PolicySet {
    let window = match settings.crypto_alert_window {
        TokenChangeWindow::OneHour => ChangeWindow::OneHour,
        TokenChangeWindow::SixHours => ChangeWindow::SixHours,
        TokenChangeWindow::TwentyFourHours => ChangeWindow::TwentyFourHours,
    };
    PolicySet {
        population: vec![PopulationPolicy {
            rule: configured_rule(
                rule_catalog::POPULATION,
                settings.population_threshold_alerts_enabled,
                settings,
            ),
            direction: ThresholdDirection::Above,
            threshold: settings.population_alert_threshold,
            hysteresis: population_hysteresis(settings.population_alert_threshold),
        }],
        prices: vec![
            PricePolicy {
                rule: configured_rule(
                    rule_catalog::TOKEN_PRICE_ABOVE,
                    settings.token_price_above_alerts_enabled,
                    settings,
                ),
                direction: ThresholdDirection::Above,
                target: settings.token_price_above_target,
                hysteresis: price_hysteresis(settings.token_price_above_target),
            },
            PricePolicy {
                rule: configured_rule(
                    rule_catalog::TOKEN_PRICE_BELOW,
                    settings.token_price_below_alerts_enabled,
                    settings,
                ),
                direction: ThresholdDirection::Below,
                target: settings.token_price_below_target,
                hysteresis: price_hysteresis(settings.token_price_below_target),
            },
        ],
        changes: vec![
            ChangePolicy {
                rule: configured_rule(
                    rule_catalog::TOKEN_CHANGE_GAIN,
                    settings.crypto_alerts_enabled && settings.token_change_gain_alerts_enabled,
                    settings,
                ),
                window,
                direction: ChangeDirection::Gain,
                threshold_percent: settings.crypto_alert_threshold,
                hysteresis_percent: rolling_hysteresis(settings.crypto_alert_threshold),
            },
            ChangePolicy {
                rule: configured_rule(
                    rule_catalog::TOKEN_CHANGE_LOSS,
                    settings.crypto_alerts_enabled && settings.token_change_loss_alerts_enabled,
                    settings,
                ),
                window,
                direction: ChangeDirection::Loss,
                threshold_percent: settings.crypto_alert_threshold,
                hysteresis_percent: rolling_hysteresis(settings.crypto_alert_threshold),
            },
        ],
        releases: Vec::new(),
        quiet_hours: Some(QuietHours::new(
            settings.advanced_alert_quiet_hours_enabled,
            settings.advanced_alert_quiet_start_minute,
            settings.advanced_alert_quiet_end_minute,
            local_timezone(),
        )),
    }
}

fn release_decision(payload: wockit::alerts::release::ReleaseAlertPayload) -> Decision {
    Decision {
        rule_id: AdvancedAlertRuleId::new(rule_catalog::RELEASE),
        fired_at: chrono::Utc::now(),
        payload: Payload::Release(ReleasePayload {
            identity: payload.identity,
            tag: payload.tag,
            name: payload.name,
            summary: payload.summary,
            url: payload.url,
            is_prerelease: payload.is_prerelease,
            published_at: payload.published_at,
        }),
    }
}

#[tauri::command]
fn autostart_enabled(autostart: tauri::State<'_, Autostart>) -> Result<bool, String> {
    autostart.is_enabled().map_err(|error| error.to_string())
}

#[tauri::command]
fn set_autostart_enabled(
    enabled: bool,
    autostart: tauri::State<'_, Autostart>,
) -> Result<bool, String> {
    autostart
        .set_enabled(enabled)
        .map_err(|error| error.to_string())?;
    autostart.is_enabled().map_err(|error| error.to_string())
}

#[tauri::command]
fn send_test_notification(
    notifications: tauri::State<'_, LiveNotifications>,
) -> Result<u32, String> {
    notifications.send_test_notification()
}

#[tauri::command]
fn settings_snapshot(state: tauri::State<'_, Arc<AppState>>) -> Settings {
    state.settings.lock().unwrap().settings().clone()
}

#[tauri::command]
fn update_setting(
    key: String,
    value: serde_json::Value,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Settings, String> {
    let mut store = state.settings.lock().unwrap();
    store.update_value(&key, value)?;
    Ok(store.settings().clone())
}

#[tauri::command]
fn clear_alert_mute(
    rule_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Settings, String> {
    if !matches!(
        rule_id.as_str(),
        rule_catalog::REALM_STATUS
            | rule_catalog::LOCAL_RECORD
            | rule_catalog::POPULATION
            | rule_catalog::TOKEN_PRICE_ABOVE
            | rule_catalog::TOKEN_PRICE_BELOW
            | rule_catalog::TOKEN_CHANGE_GAIN
            | rule_catalog::TOKEN_CHANGE_LOSS
            | rule_catalog::RELEASE
    ) {
        return Err("unknown alert rule".into());
    }
    let mut store = state.settings.lock().unwrap();
    store
        .update(DefaultsKey::AdvancedAlertMutes, |settings| {
            settings.advanced_alert_mutes.remove(&rule_id);
        })
        .map_err(|error| error.to_string())?;
    Ok(store.settings().clone())
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExportFormat {
    Csv,
    Json,
}

#[tauri::command]
fn export_history(
    format: ExportFormat,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    let samples = state
        .history
        .snapshot()
        .map_err(|error| error.to_string())?;
    let (format, title, extension) = match format {
        ExportFormat::Csv => (HistoryExportFormat::Csv, "Export CSV", "csv"),
        ExportFormat::Json => (HistoryExportFormat::Json, "Export JSON", "json"),
    };
    let Some(path) = app
        .dialog()
        .file()
        .set_title(title)
        .set_file_name(format!("woc-player-history.{extension}"))
        .add_filter(extension.to_uppercase(), &[extension])
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let path = path.into_path().map_err(|error| error.to_string())?;
    atomic_export(&path, &export_data(&samples, format)).map_err(|error| error.to_string())?;
    Ok(true)
}

fn atomic_export(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing filename"))?;
    static EXPORT_TEMP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let temporary = parent.join(format!(
        ".{}.{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        EXPORT_TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[tauri::command]
fn clear_history(state: tauri::State<'_, Arc<AppState>>) -> Result<Settings, String> {
    state.history.clear().map_err(|error| error.to_string())?;
    let mut store = state.settings.lock().unwrap();
    store
        .update(DefaultsKey::AllTimePeak, |settings| {
            settings.all_time_peak = 0;
            settings.all_time_peak_date = None;
        })
        .map_err(|error| error.to_string())?;
    Ok(store.settings().clone())
}

#[tauri::command]
fn history_persistence_error(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<String>, String> {
    state
        .history
        .persistence_error()
        .map_err(|error| error.to_string())
}
fn install_sigterm_handler(app: &AppHandle, lifecycle: LifecycleFlush) {
    let app = app.clone();
    glib::source::unix_signal_add_once(libc::SIGTERM, move || {
        if let Err(error) = lifecycle.on_sigterm() {
            eprintln!("failed to flush history on SIGTERM: {error}");
        }
        app.exit(0);
    });
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::open_dashboard(app);
        }))
        .invoke_handler(tauri::generate_handler![
            dashboard_snapshot,
            set_chart_range,
            set_candle_interval,
            dismiss_welcome,
            refresh_status,
            open_external,
            refresh_page,
            refresh_community_if_needed,
            refresh_community_feed,
            quit_app,
            autostart_enabled,
            set_autostart_enabled,
            send_test_notification,
            settings_snapshot,
            update_setting,
            clear_alert_mute,
            export_history,
            clear_history,
            history_persistence_error
        ])
        .setup(|app| {
            app.handle().plugin(tauri_plugin_dialog::init())?;
            glib::set_application_name(strings::PRODUCT_NAME);
            let state = Arc::new(AppState::open()?);
            app.manage(state.clone());
            app.manage(Autostart::discover()?);

            let lifecycle = LifecycleFlush::new(state.history.clone());
            app.manage(lifecycle.clone());
            install_sigterm_handler(app.handle(), lifecycle);

            let action_app = app.handle().clone();
            let notification_actions =
                Arc::new(AppStateNotificationActions::new(state.clone(), move || {
                    tray::open_dashboard(&action_app)
                }));
            let dispatcher = Arc::new(NotificationDispatcher::new(
                NotifyRustBackend,
                notification_actions,
            ));
            app.manage(dispatcher.clone());
            let delivery_clocks: DeliveryClocks = Arc::new(Mutex::new(HashMap::new()));

            let initial = tauri::async_runtime::block_on(state.poller.snapshot());
            let initial_mode = state
                .settings
                .lock()
                .unwrap()
                .settings()
                .menu_bar_display_mode;
            let cosmic_session = cosmic_panel::current_session_is_cosmic();
            if cosmic_panel::should_show_tray(cosmic_session, &CosmicConfig) {
                tray::build(app.handle(), &initial, initial_mode)?;
            }

            if cosmic_session {
                let policy_app = app.handle().clone();
                let policy_state = state.clone();
                tauri::async_runtime::spawn(async move {
                    let mut last_show = false;
                    let mut timer = tokio::time::interval(std::time::Duration::from_secs(1));
                    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        timer.tick().await;
                        let show = cosmic_panel::should_show_tray(true, &CosmicConfig);
                        if show == last_show {
                            continue;
                        }
                        let app = policy_app.clone();
                        if show {
                            if app.tray_by_id(tray::TRAY_ID).is_some() {
                                if let Err(error) = app.run_on_main_thread({
                                    let tray_app = app.clone();
                                    move || {
                                        if let Err(error) = tray::set_visible(&tray_app, true) {
                                            eprintln!(
                                                "failed to show generic tray fallback: {error}"
                                            );
                                        }
                                    }
                                }) {
                                    eprintln!(
                                        "failed to schedule generic tray restoration: {error}"
                                    );
                                    continue;
                                }
                                last_show = true;
                                continue;
                            }
                            let snapshot = policy_state.poller.snapshot().await;
                            let mode = policy_state
                                .settings
                                .lock()
                                .unwrap()
                                .settings()
                                .menu_bar_display_mode;
                            let tray_app = app.clone();
                            if let Err(error) = app.run_on_main_thread(move || {
                                if let Err(error) = tray::build(&tray_app, &snapshot, mode) {
                                    eprintln!("failed to restore generic tray fallback: {error}");
                                }
                            }) {
                                eprintln!("failed to schedule generic tray restoration: {error}");
                                continue;
                            }
                        } else if let Err(error) = app.run_on_main_thread({
                            let tray_app = app.clone();
                            move || {
                                if let Err(error) = tray::set_visible(&tray_app, false) {
                                    eprintln!("failed to hide duplicate generic tray: {error}");
                                }
                            }
                        }) {
                            eprintln!("failed to schedule duplicate tray hiding: {error}");
                            continue;
                        }
                        last_show = show;
                    }
                });
            }

            let app_handle = app.handle().clone();
            let history = state.history.clone();
            let settings = state.settings.clone();
            let dispatcher_for_feeds = dispatcher.clone();
            let clocks_for_feeds = delivery_clocks.clone();
            let mut poll_updates = state.poller.subscribe();
            tauri::async_runtime::spawn(async move {
                let mut recorded_success = None;
                let mut observed_status_attempt = None;
                let mut observed_quote_success = None;
                let mut status_alert_state = StatusAlertState::default();
                let mut advanced_state = PolicyState::default();
                while poll_updates.changed().await.is_ok() {
                    let snapshot = poll_updates.borrow_and_update().clone();
                    let current_settings = settings.lock().unwrap().settings().clone();
                    tray::update(
                        &app_handle,
                        &snapshot,
                        current_settings.menu_bar_display_mode,
                    );
                    if snapshot.status.last_success != recorded_success {
                        recorded_success = snapshot.status.last_success;
                        if let Some(status) = snapshot.status.value.as_ref() {
                            let _ = history.record(HistorySample {
                                date: chrono::Utc::now(),
                                count: status.players_online,
                            });
                        }
                    }
                    let alert_settings = AlertSettings {
                        status_enabled: current_settings.alerts_enabled,
                        peak_enabled: current_settings.peak_alerts_enabled,
                        crypto_enabled: current_settings.crypto_alerts_enabled,
                    };
                    if snapshot.status.last_attempt != observed_status_attempt {
                        observed_status_attempt = snapshot.status.last_attempt;
                        if observed_status_attempt.is_some() {
                            let status = snapshot.status.value.as_ref();
                            let realm = status
                                .and_then(|value| value.realm.as_deref())
                                .unwrap_or("Claudemoon");
                            let count = status.map_or(0, |value| value.players_online);
                            let observation = match snapshot.realm_availability {
                                woc_app::poller::RealmAvailability::Healthy => {
                                    StatusAlertObservation::Healthy
                                }
                                woc_app::poller::RealmAvailability::ServerReportedDown => {
                                    StatusAlertObservation::Failure {
                                        counts_toward_outage: true,
                                    }
                                }
                                woc_app::poller::RealmAvailability::Unreachable(kind) => {
                                    StatusAlertObservation::Failure {
                                        counts_toward_outage: kind
                                            .counts_toward_outage_confirmation(),
                                    }
                                }
                                woc_app::poller::RealmAvailability::Loading => continue,
                            };
                            let (next, decision) = evaluate_debounced_status(
                                status_alert_state,
                                observation,
                                2,
                                realm,
                                count,
                                alert_settings,
                            );
                            status_alert_state = next;
                            if let Some(decision) = decision {
                                deliver_if_allowed(
                                    &current_settings,
                                    legacy_rule_id(&decision),
                                    &clocks_for_feeds,
                                    || dispatcher_for_feeds.dispatch_legacy(&decision),
                                );
                            }
                            if snapshot.status.last_success == observed_status_attempt {
                                let previous_date =
                                    current_settings.all_time_peak_date.and_then(|value| {
                                        chrono::DateTime::from_timestamp(value as i64, 0)
                                    });
                                let (peak, peak_date, decision) = evaluate_peak(
                                    count,
                                    chrono::Utc::now(),
                                    current_settings.all_time_peak,
                                    previous_date,
                                    realm,
                                    alert_settings,
                                );
                                if peak != current_settings.all_time_peak {
                                    let _ = settings.lock().unwrap().update(
                                        woc_app::settings::DefaultsKey::AllTimePeak,
                                        |value| {
                                            value.all_time_peak = peak;
                                            value.all_time_peak_date =
                                                peak_date.map(|date| date.timestamp() as f64);
                                        },
                                    );
                                }
                                if let Some(decision) = decision {
                                    deliver_if_allowed(
                                        &current_settings,
                                        legacy_rule_id(&decision),
                                        &clocks_for_feeds,
                                        || dispatcher_for_feeds.dispatch_legacy(&decision),
                                    );
                                }
                            }
                        }
                    }
                    if snapshot.quote.last_success != observed_quote_success {
                        observed_quote_success = snapshot.quote.last_success;
                        if let Some(price) = snapshot
                            .quote
                            .value
                            .as_ref()
                            .and_then(|quote| quote.price.parse::<f64>().ok())
                        {
                            let (baseline, decision) = evaluate_crypto(
                                price,
                                current_settings.last_alerted_price,
                                current_settings.crypto_alert_threshold,
                                alert_settings,
                            );
                            if baseline != current_settings.last_alerted_price {
                                let _ = settings.lock().unwrap().update(
                                    woc_app::settings::DefaultsKey::LastAlertedPrice,
                                    |value| {
                                        value.last_alerted_price = baseline;
                                        value.last_alerted_price_date =
                                            Some(chrono::Utc::now().timestamp() as f64);
                                    },
                                );
                            }
                            if let Some(decision) = decision {
                                deliver_if_allowed(
                                    &current_settings,
                                    legacy_rule_id(&decision),
                                    &clocks_for_feeds,
                                    || dispatcher_for_feeds.dispatch_legacy(&decision),
                                );
                            }
                        }
                    }
                    let now = chrono::Utc::now();
                    let persisted_mutes = current_settings
                        .advanced_alert_mutes
                        .iter()
                        .map(|(key, value)| (key.clone(), *value))
                        .collect();
                    let mut mutes = RuleMutes::from_unix_seconds(&persisted_mutes, now);
                    let previous_advanced_state = advanced_state.clone();
                    let evaluation = evaluate_with_mutes(
                        &alert_policies(&current_settings),
                        advanced_state,
                        Observation {
                            observed_at: now,
                            population: snapshot
                                .status
                                .value
                                .as_ref()
                                .map(|status| status.players_online),
                            quote: snapshot.quote.value.as_ref(),
                            releases: None,
                        },
                        &mut mutes,
                    );
                    advanced_state = evaluation.state;
                    for decision in evaluation.decisions {
                        if let Err(error) = dispatcher_for_feeds.dispatch(&decision) {
                            eprintln!("failed to deliver advanced alert: {error}");
                            advanced_state.restore_delivery_clock(
                                &decision.rule_id,
                                &previous_advanced_state,
                            );
                        }
                    }
                    let _ = app_handle.emit(woc_app::app_state::events::STATUS_CHANGED, ());
                    let _ = app_handle.emit(woc_app::app_state::events::MARKET_CHANGED, ());
                }
            });

            let app_handle = app.handle().clone();
            let mut community_updates = state.community.subscribe();
            tauri::async_runtime::spawn(async move {
                while community_updates.changed().await.is_ok() {
                    let _ = app_handle.emit(woc_app::app_state::events::COMMUNITY_CHANGED, ());
                }
            });

            if let Some(settings_changes) = state.take_settings_changes() {
                let app_handle = app.handle().clone();
                let state_for_settings = state.clone();
                let dispatcher_for_settings = dispatcher.clone();
                let clocks_for_settings = delivery_clocks.clone();
                std::thread::spawn(move || {
                    while let Ok(change) = settings_changes.recv() {
                        let check_now = state_for_settings.apply_settings(&change.settings);
                        let _ = app_handle.emit(woc_app::app_state::events::SETTINGS_CHANGED, ());
                        tauri::async_runtime::block_on(async {
                            state_for_settings
                                .poller
                                .set_candle_interval(woc_app::app_state::candle_interval(
                                    change.settings.crypto_chart_interval,
                                ))
                                .await;
                            let snapshot = state_for_settings.poller.snapshot().await;
                            tray::update(
                                &app_handle,
                                &snapshot,
                                change.settings.menu_bar_display_mode,
                            );
                            if check_now {
                                if let Some(payload) =
                                    state_for_settings.release_monitor.tick().await
                                {
                                    let decision = release_decision(payload);
                                    if deliver_if_allowed(
                                        &change.settings,
                                        rule_catalog::RELEASE,
                                        &clocks_for_settings,
                                        || dispatcher_for_settings.dispatch(&decision),
                                    ) {
                                        let _ = app_handle
                                            .emit(woc_app::app_state::events::RELEASE_ALERT, ());
                                    }
                                }
                            }
                        });
                    }
                });
            }

            let release_app = app.handle().clone();
            let dispatcher_for_releases = dispatcher.clone();
            let clocks_for_releases = delivery_clocks.clone();
            let settings_for_releases = state.settings.clone();
            tauri::async_runtime::spawn(async move {
                let _tasks = state.poller.start();
                if state.release_monitor.start() {
                    if let Some(payload) = state.release_monitor.tick().await {
                        let current = settings_for_releases.lock().unwrap().settings().clone();
                        let decision = release_decision(payload);
                        if deliver_if_allowed(
                            &current,
                            rule_catalog::RELEASE,
                            &clocks_for_releases,
                            || dispatcher_for_releases.dispatch(&decision),
                        ) {
                            let _ = release_app.emit(woc_app::app_state::events::RELEASE_ALERT, ());
                        }
                    }
                }
                let mut timer = tokio::time::interval(woc_app::community::RELEASE_MONITOR_INTERVAL);
                timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                timer.tick().await;
                loop {
                    timer.tick().await;
                    if let Some(payload) = state.release_monitor.tick().await {
                        let current = settings_for_releases.lock().unwrap().settings().clone();
                        let decision = release_decision(payload);
                        if deliver_if_allowed(
                            &current,
                            rule_catalog::RELEASE,
                            &clocks_for_releases,
                            || dispatcher_for_releases.dispatch(&decision),
                        ) {
                            let _ = release_app.emit(woc_app::app_state::events::RELEASE_ALERT, ());
                        }
                    }
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build tauri application")
        .run(|app, event| {
            if let RunEvent::WindowEvent { label, event, .. } = &event {
                if label == tray::DASHBOARD_LABEL && matches!(event, tauri::WindowEvent::Destroyed)
                {
                    tray::mark_dashboard_hidden(app);
                    if let Err(error) = app.state::<LifecycleFlush>().on_dashboard_close() {
                        eprintln!("failed to flush history on dashboard close: {error}");
                    }
                }
            }
            if let RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                } else if let Err(error) = app.state::<LifecycleFlush>().on_quit() {
                    eprintln!("failed to flush history on quit: {error}");
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_market_links_keep_the_dex_allowlist_and_exact_gecko_attribution() {
        assert!(is_allowed_external_url(
            "https://dexscreener.com/solana/5we9yjzpeqxcyl4jn9khjtsr48xzyh47xtar9kg3wy1p"
        ));
        assert!(is_allowed_external_url(
            "https://www.geckoterminal.com/solana/pools/5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p"
        ));
        assert!(!is_allowed_external_url("http://dexscreener.com/pair"));
        assert!(!is_allowed_external_url(
            "https://dexscreener.com.evil.example/pair"
        ));
        assert!(!is_allowed_external_url(
            "https://www.geckoterminal.com/solana/pools/wrong"
        ));
    }

    #[test]
    fn rolling_change_rules_require_the_crypto_master_toggle() {
        let settings = Settings {
            crypto_alerts_enabled: false,
            token_change_gain_alerts_enabled: true,
            token_change_loss_alerts_enabled: true,
            ..Settings::default()
        };
        let policies = alert_policies(&settings);
        assert!(policies.changes.iter().all(|policy| !policy.rule.enabled));

        let enabled = Settings {
            crypto_alerts_enabled: true,
            ..settings
        };
        assert!(alert_policies(&enabled)
            .changes
            .iter()
            .all(|policy| policy.rule.enabled));
    }

    #[test]
    fn persisted_mute_blocks_exactly_one_rule() {
        let mut settings = Settings {
            advanced_alert_cooldown: 0.0,
            ..Settings::default()
        };
        settings.advanced_alert_mutes.insert(
            rule_catalog::REALM_STATUS.into(),
            (chrono::Utc::now().timestamp() + 3_600) as f64,
        );
        let clocks: DeliveryClocks = Arc::new(Mutex::new(HashMap::new()));
        assert!(!deliver_if_allowed(
            &settings,
            rule_catalog::REALM_STATUS,
            &clocks,
            || Ok::<_, String>(())
        ));
        assert!(deliver_if_allowed(
            &settings,
            rule_catalog::LOCAL_RECORD,
            &clocks,
            || Ok::<_, String>(())
        ));
    }

    #[test]
    fn shared_cooldown_consumes_second_delivery_for_one_rule() {
        let settings = Settings {
            advanced_alert_cooldown: 3_600.0,
            ..Settings::default()
        };
        let clocks: DeliveryClocks = Arc::new(Mutex::new(HashMap::new()));
        assert!(deliver_if_allowed(
            &settings,
            rule_catalog::REALM_STATUS,
            &clocks,
            || Ok::<_, String>(())
        ));
        assert!(!deliver_if_allowed(
            &settings,
            rule_catalog::REALM_STATUS,
            &clocks,
            || Ok::<_, String>(())
        ));
        assert!(deliver_if_allowed(
            &settings,
            rule_catalog::LOCAL_RECORD,
            &clocks,
            || Ok::<_, String>(())
        ));
    }

    #[test]
    fn failed_notification_does_not_advance_delivery_clock() {
        let settings = Settings {
            advanced_alert_cooldown: 3_600.0,
            ..Settings::default()
        };
        let clocks: DeliveryClocks = Arc::new(Mutex::new(HashMap::new()));
        assert!(!deliver_if_allowed(
            &settings,
            rule_catalog::REALM_STATUS,
            &clocks,
            || Err::<(), _>("daemon unavailable".into())
        ));
        assert!(deliver_if_allowed(
            &settings,
            rule_catalog::REALM_STATUS,
            &clocks,
            || Ok::<_, String>(())
        ));
    }
}
