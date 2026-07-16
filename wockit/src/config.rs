//! Single home for endpoints, timeouts, poll defaults, and the data/analytics constants.
//!
//! Port of the macOS `AppConfig` (Config/AppConfig.swift), plus the `PollInterval`
//! (Config/PollInterval.swift) and `AlertCooldownOption` (Config/AlertCooldownOption.swift)
//! option enums. No URL or numeric literal for these concerns should appear in services,
//! the store, or analytics.
//!
//! Omitted from the Swift original: `AppConfig.notificationAuthorizationOptions` is a
//! macOS `UserNotifications` type with no Linux equivalent (notification behavior is the
//! shell's concern here).

use crate::strings;

/// Secure destinations whose identities are asserted by FormattingTests.swift.
pub mod app_links {
    pub const APP_REPOSITORY: &str = "https://github.com/FernandoX7/woc-widget-linux";
    pub const APP_PRIVACY: &str =
        "https://github.com/FernandoX7/woc-widget-linux/blob/main/PRIVACY.md";
    pub const APP_LICENSE: &str =
        "https://github.com/FernandoX7/woc-widget-linux/blob/main/LICENSE";
    pub const APP_SUPPORT: &str = "https://github.com/FernandoX7/woc-widget-linux/issues";
    pub const GAME_REPOSITORY: &str = "https://github.com/levy-street/world-of-claudecraft";
}

/// Feed endpoints and request hardening. Swift parity: `AppConfig.API`.
///
/// Swift also pins `cachePolicy = .reloadIgnoringLocalCacheData`; the cache-parity
/// rationale for the Rust side lives as a doc comment on [`crate::http`].
pub mod api {
    /// Player count / realm status.
    pub const STATUS_URL: &str = "https://worldofclaudecraft.com/api/status";

    /// $WOC price comes from a DexScreener pair. Kept as base + chain + pair address so
    /// the pair is named rather than buried in a URL.
    pub const DEX_BASE: &str = "https://api.dexscreener.com/latest/dex/pairs";
    /// DexScreener chain segment for the $WOC pair.
    pub const DEX_CHAIN: &str = "solana";
    /// Lowercase lookup value accepted by DexScreener. The service separately validates
    /// the canonical, case-sensitive address returned in the payload before trusting a
    /// quote. Never normalize this or [`DEX_CANONICAL_PAIR_ADDRESS`] — both spellings are
    /// correct as-is.
    pub const DEX_PAIR_ADDRESS: &str = "5we9yjzpeqxcyl4jn9khjtsr48xzyh47xtar9kg3wy1p";
    /// Canonical, case-sensitive pair address as returned in the DexScreener payload.
    pub const DEX_CANONICAL_PAIR_ADDRESS: &str = "5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p";
    /// $WOC token mint address on Solana.
    pub const DEX_TOKEN_ADDRESS: &str = "3WjLscH2JsXLEFJZRA9z8ti8yRGxWGKbqymPd7UicRth";

    /// The DexScreener quote endpoint, joined exactly like the Swift `cryptoURL`.
    pub fn crypto_url() -> url::Url {
        url::Url::parse(&format!("{DEX_BASE}/{DEX_CHAIN}/{DEX_PAIR_ADDRESS}"))
            .expect("static DexScreener endpoint components form a valid URL")
    }

    /// Real OHLC candles for the $WOC chart come from GeckoTerminal (CoinGecko on-chain).
    /// Free, no API key.
    pub const GECKO_BASE: &str = "https://api.geckoterminal.com/api/v2/networks";
    /// GeckoTerminal network segment for the $WOC pool.
    pub const GECKO_NETWORK: &str = "solana";
    /// The pool address is CASE-SENSITIVE here — DexScreener lowercases it, which
    /// GeckoTerminal 404s on; this is the exact mixed case the API expects (it equals
    /// [`DEX_CANONICAL_PAIR_ADDRESS`]). Never normalize.
    pub const GECKO_POOL: &str = "5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p";

    /// Applied to every request. Swift: `requestTimeout: TimeInterval = 12` (seconds).
    pub const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);
    /// Reject unexpectedly large public-feed payloads before decoding. Normal responses
    /// are measured in kilobytes; this ceiling leaves ample headroom for release notes
    /// while bounding schema abuse and accidental upstream error documents.
    pub const MAXIMUM_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

    /// DexScreener's API controls the pair URL rendered as a clickable market action.
    /// Keep that navigation on HTTPS and the expected service domain even if an upstream
    /// payload or injected service is compromised.
    /// Swift parity: `AppConfig.API.validatedMarketURL(_:)`.
    pub fn validated_market_url(candidate: Option<&str>) -> Option<url::Url> {
        let url = url::Url::parse(candidate?).ok()?;
        if !url.scheme().eq_ignore_ascii_case("https") {
            return None;
        }
        let host = url.host_str()?.to_ascii_lowercase();
        if host == "dexscreener.com" || host.ends_with(".dexscreener.com") {
            Some(url)
        } else {
            None
        }
    }
}

/// Crypto chart constants. Swift parity: `AppConfig.Crypto`.
pub mod crypto {
    /// Quote currency for prices and candles.
    pub const CURRENCY_CODE: &str = "USD";
    /// How many candles to request/show. Sized for a glanceable dashboard chart: dense
    /// enough to read a trend, sparse enough that each candle is still a candle.
    /// GeckoTerminal caps `limit` at 1000.
    pub const CANDLE_COUNT: usize = 60;
    /// Candles are only refreshed while the dashboard is visible (seconds). Spot polling
    /// remains on its own selected cadence; this slower floor avoids downloading 60 bars
    /// every minute.
    pub const VISIBLE_CANDLE_REFRESH_SECONDS: f64 = 300.0;
}

/// Polling defaults and floors. Swift parity: `AppConfig.Poll`. All values are seconds
/// unless noted.
pub mod poll {
    /// Default player-status poll interval (seconds).
    pub const DEFAULT_PLAYER_SECONDS: f64 = 60.0;
    /// Default crypto-quote poll interval (seconds).
    pub const DEFAULT_CRYPTO_SECONDS: f64 = 60.0;
    /// Floor a corrupted/0/negative persisted interval clamps to, so a bad value can't
    /// produce a tight-loop timer (seconds).
    pub const MINIMUM_SECONDS: f64 = 10.0;
    /// Slack each repeating poll timer is allowed (as a fraction of its interval) so the
    /// platform can coalesce the two pollers' wakeups instead of firing each at an exact
    /// instant. Applied off the live interval, so an interval change picks up the new
    /// tolerance automatically.
    pub const TIMER_TOLERANCE_FRACTION: f64 = 0.1;
}

/// History persistence + retention. Swift parity: `AppConfig.History`.
pub mod history {
    /// Data directory name under `$XDG_DATA_HOME`.
    ///
    /// DELIBERATE divergence from macOS ("WoCWidget"): the frozen Linux contract in
    /// CLAUDE.md is `$XDG_DATA_HOME/woc-widget/history.json`, matching Linux lowercase
    /// directory conventions.
    pub const DIRECTORY_NAME: &str = "woc-widget";
    /// History file name inside [`DIRECTORY_NAME`].
    pub const FILE_NAME: &str = "history.json";
    /// Samples older than this are pruned (seconds; 7 days). The chart range never
    /// exceeds this window.
    pub const RETENTION_WINDOW: f64 = 604_800.0;
    /// The plot is downsampled so it never exceeds this many points; this same value is
    /// the bucket-coarsening divisor in the history analytics. Must stay `f64` so
    /// `range_seconds / CHART_MAX_POINTS` is byte-identical to the Swift
    /// `range.seconds / 300`.
    pub const CHART_MAX_POINTS: f64 = 300.0;
    /// Writes are coalesced — the array is flushed to disk after this many new samples
    /// or [`FLUSH_EVERY_SECONDS`] (whichever first), plus on quit.
    pub const FLUSH_EVERY_SAMPLES: usize = 12;
    /// Time-based flush companion to [`FLUSH_EVERY_SAMPLES`] (seconds).
    pub const FLUSH_EVERY_SECONDS: f64 = 120.0;
    /// Small tolerance for clock corrections (seconds); samples farther in the future
    /// are corrupt.
    pub const FUTURE_SAMPLE_TOLERANCE: f64 = 300.0;
    /// Population-change context shown in the overview (seconds). A baseline must be
    /// close to the target time; an observation from before a sleep/wake gap must never
    /// masquerade as a 30-minute comparison.
    pub const SHORT_CHANGE_WINDOW: f64 = 1_800.0;
    /// Minimum baseline tolerance for the short-change comparison (seconds).
    pub const SHORT_CHANGE_MINIMUM_TOLERANCE: f64 = 60.0;
    /// Baseline tolerance also scales with the live poll interval by this multiplier.
    pub const SHORT_CHANGE_POLL_TOLERANCE_MULTIPLIER: f64 = 2.0;
    /// A spacing beyond this multiple of the expected bucket/poll cadence starts a new
    /// chart segment instead of drawing a line across observations that do not exist.
    pub const CHART_GAP_MULTIPLIER: f64 = 1.75;
    /// Realm-rhythm percentile copy needs enough independent observations to avoid
    /// turning a few startup samples into a confident-sounding comparison.
    pub const RHYTHM_MINIMUM_SAMPLES: usize = 30;
}

/// The on-open refresh fires when the last success is older than this (seconds).
/// Swift parity: `AppConfig.stalenessWindow`.
pub const STALENESS_WINDOW: f64 = 20.0;

/// How long a successful quote remains eligible for a live panel presentation.
///
/// The window covers the configured market cadence, the scheduler's ten-percent
/// tolerance, and one request timeout. The shared staleness window remains the floor for
/// shorter cadences. A completed failed or invalid refresh still overrides this deadline
/// immediately at the feed-state layer.
pub fn panel_price_freshness(market_interval: std::time::Duration) -> std::time::Duration {
    let scheduled_refresh = market_interval
        .mul_f64(1.0 + poll::TIMER_TOLERANCE_FRACTION)
        .saturating_add(api::REQUEST_TIMEOUT);
    scheduled_refresh.max(std::time::Duration::from_secs_f64(STALENESS_WINDOW))
}

/// Crypto price-move alert constants. Swift parity: `AppConfig.CryptoAlert`.
pub mod crypto_alert {
    /// Default percent-move threshold.
    pub const DEFAULT_THRESHOLD_PERCENT: f64 = 10.0;
    /// Settings slider bounds for the threshold (percent).
    pub const SLIDER_RANGE: std::ops::RangeInclusive<f64> = 1.0..=50.0;
    /// Settings slider step (percent).
    pub const SLIDER_STEP: f64 = 1.0;
    /// A persisted baseline older than this (seconds; 24 h) is reseeded from the next
    /// quote without alerting. This prevents an app reopened weeks later from announcing
    /// an ancient move as new.
    pub const MAXIMUM_BASELINE_AGE: f64 = 86_400.0;
}

/// Realm-outage alert constants. Swift parity: `AppConfig.Alert`.
pub mod alert {
    /// Require repeated remote-failure observations before claiming the realm looks down.
    pub const OUTAGE_CONFIRMATION_POLLS: u32 = 2;
}

/// Release-feed alert constants. Swift parity: `AppConfig.ReleaseAlert`. New releases are
/// rare, so their alert feed uses a deliberately slow, highly tolerant cadence
/// independent of the live player and market pollers.
pub mod release_alert {
    /// Release-feed poll interval (seconds; 30 min).
    pub const POLL_SECONDS: f64 = 1_800.0;
    /// Release-feed timer tolerance (seconds; 3 min).
    pub const TIMER_TOLERANCE: f64 = 180.0;
    /// A wider snapshot prevents several releases published between checks from being
    /// rediscovered later; the alert policy consumes all identities and announces only
    /// newest.
    pub const FETCH_LIMIT: usize = 10;
}

/// Alerts 2.0 constants and normalizers. Swift parity: `AppConfig.AdvancedAlert`.
pub mod advanced_alert {
    use super::AlertCooldownOption;

    /// Default population-crossing threshold (players).
    pub const DEFAULT_POPULATION_THRESHOLD: i64 = 125;
    /// Valid population-threshold bounds (players).
    pub const POPULATION_RANGE: std::ops::RangeInclusive<i64> = 1..=10_000;
    /// Population stepper increment (players).
    pub const POPULATION_STEP: i64 = 5;

    // Disabled until explicitly enabled; these are only sensible starting points for the
    // target fields, not product claims about where the market will trade.
    /// Default "price above" target (USD).
    pub const DEFAULT_PRICE_ABOVE_TARGET: f64 = 0.001;
    /// Default "price below" target (USD).
    pub const DEFAULT_PRICE_BELOW_TARGET: f64 = 0.00025;
    /// Smallest accepted price target (USD).
    pub const MINIMUM_PRICE_TARGET: f64 = 0.000_000_000_001;
    /// Largest accepted price target (USD).
    pub const MAXIMUM_PRICE_TARGET: f64 = 1_000_000.0;

    /// Duration a per-rule mute lasts (seconds; 1 h).
    pub const MUTE_DURATION: f64 = 3_600.0;
    /// Default quiet-hours start, minutes after midnight (22:00).
    pub const DEFAULT_QUIET_START_MINUTE: u32 = 1_320;
    /// Default quiet-hours end, minutes after midnight (07:00).
    pub const DEFAULT_QUIET_END_MINUTE: u32 = 420;

    /// Default shared cooldown (seconds), computed from the one-hour option exactly like
    /// the Swift `defaultCooldown`.
    pub fn default_cooldown() -> f64 {
        AlertCooldownOption::OneHour.seconds()
    }

    /// All supported cooldown choices in seconds, in declaration order. Swift parity:
    /// `cooldownOptions = AlertCooldownOption.allCases.map(\.seconds)`.
    pub fn cooldown_options() -> Vec<f64> {
        AlertCooldownOption::all_cases()
            .iter()
            .map(|option| option.seconds())
            .collect()
    }

    /// Clamps a persisted population threshold into [`POPULATION_RANGE`].
    /// Swift parity: `AppConfig.AdvancedAlert.normalizePopulation(_:)`.
    pub fn normalize_population(value: i64) -> i64 {
        value.clamp(*POPULATION_RANGE.start(), *POPULATION_RANGE.end())
    }

    /// Falls back if the value is non-finite or non-positive, else clamps into the
    /// supported target bounds.
    /// Swift parity: `AppConfig.AdvancedAlert.normalizePriceTarget(_:fallback:)`.
    pub fn normalize_price_target(value: f64, fallback: f64) -> f64 {
        if !value.is_finite() || value <= 0.0 {
            return fallback;
        }
        value.clamp(MINIMUM_PRICE_TARGET, MAXIMUM_PRICE_TARGET)
    }

    /// Snaps a persisted cooldown onto the nearest supported option by absolute
    /// distance; non-finite values take the default.
    /// Swift parity: `AppConfig.AdvancedAlert.normalizeCooldown(_:)`.
    pub fn normalize_cooldown(value: f64) -> f64 {
        if !value.is_finite() {
            return default_cooldown();
        }
        // Swift's Sequence.min(by:) keeps the FIRST minimal element on equal distances;
        // replicate that by replacing only on strictly smaller distance.
        let mut best: Option<f64> = None;
        for option in cooldown_options() {
            match best {
                Some(current) if (option - value).abs() >= (current - value).abs() => {}
                _ => best = Some(option),
            }
        }
        best.unwrap_or_else(default_cooldown)
    }
}

/// The selectable poll intervals, so the settings pickers iterate a typed list instead of
/// ad-hoc inline arrays. The picker binds to a seconds value, so each option tags its
/// [`PollInterval::seconds`]. Swift parity: Config/PollInterval.swift.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PollInterval {
    /// 10 seconds.
    TenSeconds = 10,
    /// 30 seconds.
    ThirtySeconds = 30,
    /// 1 minute.
    OneMinute = 60,
    /// 5 minutes.
    FiveMinutes = 300,
    /// 15 minutes.
    FifteenMinutes = 900,
}

impl PollInterval {
    /// The interval length in seconds.
    pub fn seconds(self) -> f64 {
        self as i64 as f64
    }

    /// Compact human label ("30s", "1m", ...), shared with the chart intervals via
    /// [`strings::compact_duration`].
    pub fn label(self) -> String {
        strings::compact_duration(self as i64)
    }

    /// Player-refresh options: 10s / 30s / 1m / 5m.
    pub const fn player_options() -> [PollInterval; 4] {
        [
            Self::TenSeconds,
            Self::ThirtySeconds,
            Self::OneMinute,
            Self::FiveMinutes,
        ]
    }

    /// Crypto-refresh options: 30s / 1m / 5m / 15m.
    pub const fn crypto_options() -> [PollInterval; 4] {
        [
            Self::ThirtySeconds,
            Self::OneMinute,
            Self::FiveMinutes,
            Self::FifteenMinutes,
        ]
    }

    /// Coerces persisted/caller-provided numeric values onto an actual picker option.
    /// Values below the minimum land on the minimum; other unsupported finite values
    /// choose the nearest option (ties prefer the lower-power, longer interval).
    /// NaN/Inf fall back to the supplied default.
    /// Swift parity: `PollInterval.normalize(_:options:default:)`.
    pub fn normalize(seconds: f64, options: &[PollInterval], default: PollInterval) -> f64 {
        if !seconds.is_finite() || options.is_empty() {
            return default.seconds();
        }
        // Mirrors the Swift min(by:) predicate: replace on strictly smaller distance, or
        // on an exact distance tie when the candidate is the larger interval.
        let mut best: Option<PollInterval> = None;
        for &candidate in options {
            let Some(current) = best else {
                best = Some(candidate);
                continue;
            };
            let candidate_distance = (candidate.seconds() - seconds).abs();
            let current_distance = (current.seconds() - seconds).abs();
            let replaces = candidate_distance < current_distance
                || (candidate_distance == current_distance
                    && candidate.seconds() > current.seconds());
            if replaces {
                best = Some(candidate);
            }
        }
        best.map_or_else(|| default.seconds(), PollInterval::seconds)
    }
}

/// Supported persisted cooldown choices. Settings and normalization share this list so a
/// picker cannot offer a value the store will silently coerce to a different one.
/// Swift parity: Config/AlertCooldownOption.swift.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AlertCooldownOption {
    /// No cooldown.
    Off,
    /// 15 minutes.
    FifteenMinutes,
    /// 1 hour.
    OneHour,
    /// 4 hours.
    FourHours,
    /// 1 day.
    OneDay,
}

impl AlertCooldownOption {
    /// The cooldown length in seconds.
    pub fn seconds(self) -> f64 {
        match self {
            Self::Off => 0.0,
            Self::FifteenMinutes => 900.0,
            Self::OneHour => 3_600.0,
            Self::FourHours => 14_400.0,
            Self::OneDay => 86_400.0,
        }
    }

    /// Every case in declaration order. Swift parity: `AlertCooldownOption.allCases`.
    pub const fn all_cases() -> [AlertCooldownOption; 5] {
        [
            Self::Off,
            Self::FifteenMinutes,
            Self::OneHour,
            Self::FourHours,
            Self::OneDay,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn companion_and_game_destinations_stay_distinct_and_secure() {
        let companion = [
            app_links::APP_REPOSITORY,
            app_links::APP_PRIVACY,
            app_links::APP_LICENSE,
            app_links::APP_SUPPORT,
        ];
        for raw in companion {
            let parsed = url::Url::parse(raw).unwrap();
            assert_eq!(parsed.scheme(), "https");
        }
        let app = url::Url::parse(app_links::APP_REPOSITORY).unwrap();
        let game = url::Url::parse(app_links::GAME_REPOSITORY).unwrap();
        assert_eq!(app.host_str(), Some("github.com"));
        assert_eq!(app.path(), "/FernandoX7/woc-widget-linux");
        assert_ne!(app, game);
        assert_eq!(game.path(), "/levy-street/world-of-claudecraft");
    }

    // No dedicated Swift test exists for validatedMarketURL; the accept/reject matrix
    // below exercises the AppConfig.API.validatedMarketURL guard ported verbatim.
    #[test]
    fn validated_market_url_accepts_https_dexscreener_hosts() {
        let accepted = ["https://dexscreener.com/x", "https://www.dexscreener.com/x"];
        for candidate in accepted {
            let url = api::validated_market_url(Some(candidate));
            assert_eq!(
                url.map(String::from),
                Some(candidate.to_string()),
                "expected {candidate} to validate"
            );
        }
        // Scheme/host case is normalized by the URL parser before the comparison.
        let uppercase = api::validated_market_url(Some("HTTPS://DEXSCREENER.COM/x"));
        assert_eq!(
            uppercase.map(String::from),
            Some("https://dexscreener.com/x".to_string())
        );
    }

    #[test]
    fn validated_market_url_rejects_everything_else() {
        let rejected = [
            "http://dexscreener.com/x",
            "https://evil.com/x",
            "https://notdexscreener.com/x",
            "https://dexscreener.com.evil.com/x",
            // Userinfo confusion: the WHATWG parser reads the real host as evil.com.
            "https://dexscreener.com@evil.com/x",
            "javascript:alert(1)",
            "data:text/html,x",
            "https://1.2.3.4/x",
            "https://[::1]/x",
            // IDNA lookalikes map to xn-- ASCII hosts, which never match the allowlist.
            "https://xn--dexscrener-77a.com/x",
            "not a url",
        ];
        for candidate in rejected {
            assert_eq!(
                api::validated_market_url(Some(candidate)),
                None,
                "expected {candidate} to be rejected"
            );
        }
        assert_eq!(api::validated_market_url(None), None);
    }

    #[test]
    fn crypto_url_joins_base_chain_and_lowercase_pair() {
        assert_eq!(
            api::crypto_url().as_str(),
            "https://api.dexscreener.com/latest/dex/pairs/solana/\
             5we9yjzpeqxcyl4jn9khjtsr48xzyh47xtar9kg3wy1p"
        );
    }

    #[test]
    fn pair_addresses_keep_their_exact_case() {
        // The lowercase lookup and the mixed-case canonical/Gecko pool are BOTH correct;
        // neither may ever be normalized toward the other.
        assert_eq!(
            api::DEX_PAIR_ADDRESS,
            api::DEX_CANONICAL_PAIR_ADDRESS.to_lowercase()
        );
        assert_ne!(api::DEX_PAIR_ADDRESS, api::DEX_CANONICAL_PAIR_ADDRESS);
        assert_eq!(api::GECKO_POOL, api::DEX_CANONICAL_PAIR_ADDRESS);
    }

    // Swift parity: FormattingTests.swift > CompactDurationTests >
    // domainIntervalsShareLocalizedCompactLabels
    #[test]
    fn poll_interval_labels_share_localized_compact_labels() {
        assert_eq!(PollInterval::ThirtySeconds.label(), "30s");
        assert_eq!(PollInterval::TenSeconds.label(), "10s");
        assert_eq!(PollInterval::OneMinute.label(), "1m");
        assert_eq!(PollInterval::FiveMinutes.label(), "5m");
        assert_eq!(PollInterval::FifteenMinutes.label(), "15m");
    }

    #[test]
    fn panel_price_freshness_covers_every_supported_crypto_cadence() {
        let cases = [
            (PollInterval::ThirtySeconds, 45),
            (PollInterval::OneMinute, 78),
            (PollInterval::FiveMinutes, 342),
            (PollInterval::FifteenMinutes, 1_002),
        ];
        for (interval, expected_seconds) in cases {
            assert_eq!(
                panel_price_freshness(std::time::Duration::from_secs_f64(interval.seconds())),
                std::time::Duration::from_secs(expected_seconds),
                "{interval:?}"
            );
        }
    }

    #[test]
    fn panel_price_freshness_keeps_the_twenty_second_floor() {
        assert_eq!(
            panel_price_freshness(std::time::Duration::from_secs(1)),
            std::time::Duration::from_secs(20)
        );
    }

    // Swift parity: FormattingTests.swift > CompactDurationTests >
    // domainIntervalsShareLocalizedCompactLabels
    #[test]
    fn cooldown_options_match_alert_cooldown_option_all_cases() {
        assert_eq!(
            advanced_alert::cooldown_options(),
            vec![0.0, 900.0, 3_600.0, 14_400.0, 86_400.0]
        );
        let from_cases: Vec<f64> = AlertCooldownOption::all_cases()
            .iter()
            .map(|option| option.seconds())
            .collect();
        assert_eq!(advanced_alert::cooldown_options(), from_cases);
    }

    // Swift parity: StoreTests.swift > StoreTests > normalizesCorruptedPollIntervalsOnLoad
    // (2.0 -> tenSeconds for the player options, 0.0 -> thirtySeconds for crypto).
    #[test]
    fn normalize_clamps_out_of_range_values_onto_the_minimum_option() {
        assert_eq!(
            PollInterval::normalize(
                2.0,
                &PollInterval::player_options(),
                PollInterval::OneMinute
            ),
            10.0
        );
        assert_eq!(
            PollInterval::normalize(
                0.0,
                &PollInterval::crypto_options(),
                PollInterval::OneMinute
            ),
            30.0
        );
    }

    // Swift parity: StoreTests.swift > StoreTests >
    // normalizesPersistedThresholdAndUnsupportedPollOptions (121 -> 60, infinity -> default).
    #[test]
    fn normalize_snaps_unsupported_finite_values_to_the_nearest_option() {
        assert_eq!(
            PollInterval::normalize(
                121.0,
                &PollInterval::player_options(),
                PollInterval::OneMinute
            ),
            60.0
        );
        assert_eq!(
            PollInterval::normalize(
                f64::INFINITY,
                &PollInterval::crypto_options(),
                PollInterval::OneMinute
            ),
            60.0
        );
    }

    // The Swift doc comment on PollInterval.normalize pins tie behavior: "ties prefer
    // the lower-power, longer interval" (min(by:) with lhs.seconds > rhs.seconds on ties).
    #[test]
    fn normalize_prefers_the_larger_interval_on_an_exact_distance_tie() {
        assert_eq!(
            PollInterval::normalize(
                45.0,
                &PollInterval::player_options(),
                PollInterval::TenSeconds
            ),
            60.0
        );
    }

    #[test]
    fn normalize_falls_back_to_the_default_for_nan_and_empty_options() {
        assert_eq!(
            PollInterval::normalize(
                f64::NAN,
                &PollInterval::player_options(),
                PollInterval::TenSeconds
            ),
            10.0
        );
        assert_eq!(
            PollInterval::normalize(60.0, &[], PollInterval::FifteenMinutes),
            900.0
        );
    }

    // No dedicated Swift test; ports AppConfig.AdvancedAlert.normalizePopulation exactly.
    #[test]
    fn normalize_population_clamps_into_range() {
        assert_eq!(advanced_alert::normalize_population(0), 1);
        assert_eq!(advanced_alert::normalize_population(-125), 1);
        assert_eq!(advanced_alert::normalize_population(125), 125);
        assert_eq!(advanced_alert::normalize_population(10_000), 10_000);
        assert_eq!(advanced_alert::normalize_population(20_000), 10_000);
    }

    // No dedicated Swift test; ports AppConfig.AdvancedAlert.normalizePriceTarget exactly.
    #[test]
    fn normalize_price_target_falls_back_then_clamps() {
        let fallback = advanced_alert::DEFAULT_PRICE_ABOVE_TARGET;
        assert_eq!(
            advanced_alert::normalize_price_target(f64::NAN, fallback),
            fallback
        );
        assert_eq!(
            advanced_alert::normalize_price_target(f64::INFINITY, fallback),
            fallback
        );
        assert_eq!(
            advanced_alert::normalize_price_target(f64::NEG_INFINITY, fallback),
            fallback
        );
        assert_eq!(
            advanced_alert::normalize_price_target(-0.5, fallback),
            fallback
        );
        assert_eq!(
            advanced_alert::normalize_price_target(0.0, fallback),
            fallback
        );
        assert_eq!(
            advanced_alert::normalize_price_target(1e-15, fallback),
            advanced_alert::MINIMUM_PRICE_TARGET
        );
        assert_eq!(
            advanced_alert::normalize_price_target(2_000_000.0, fallback),
            advanced_alert::MAXIMUM_PRICE_TARGET
        );
        assert_eq!(advanced_alert::normalize_price_target(0.5, fallback), 0.5);
    }

    // No dedicated Swift test; ports AppConfig.AdvancedAlert.normalizeCooldown exactly,
    // including Swift min(by:)'s first-minimal-wins behavior on exact midpoints.
    #[test]
    fn normalize_cooldown_snaps_to_the_nearest_option() {
        assert_eq!(advanced_alert::normalize_cooldown(f64::NAN), 3_600.0);
        assert_eq!(advanced_alert::normalize_cooldown(f64::INFINITY), 3_600.0);
        assert_eq!(advanced_alert::normalize_cooldown(-100.0), 0.0);
        assert_eq!(advanced_alert::normalize_cooldown(1_000.0), 900.0);
        assert_eq!(advanced_alert::normalize_cooldown(100_000.0), 86_400.0);
        // Exact midpoints: Swift's min(by:) keeps the earlier (smaller) option.
        assert_eq!(advanced_alert::normalize_cooldown(450.0), 0.0);
        assert_eq!(advanced_alert::normalize_cooldown(2_250.0), 900.0);
        assert_eq!(advanced_alert::normalize_cooldown(9_000.0), 3_600.0);
        assert_eq!(advanced_alert::normalize_cooldown(50_400.0), 14_400.0);
    }

    #[test]
    fn default_cooldown_is_the_one_hour_option() {
        assert_eq!(
            advanced_alert::default_cooldown(),
            AlertCooldownOption::OneHour.seconds()
        );
    }
}
