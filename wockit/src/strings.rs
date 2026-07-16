//! User-facing strings for the wockit domain layer.
//!
//! Port of the macOS AppText string helpers used by domain types, plus the
//! FetchError.friendlyMessage table. English-only for now, but every string keeps the
//! macOS String Catalog key so a future localization pass swaps t() for a real lookup
//! without touching call sites.

/// Localization shim: returns the English fallback, keyed identically to the macOS
/// String Catalog ("Localizable" table).
fn t(_key: &str, fallback: &str) -> String {
    fallback.to_string()
}

use crate::formatting::{chart_price, RealmAvailability};

/// Non-localizable status glyphs and no-value marker shared by panel/count formatting.
pub const STATUS_LOADING: &str = "🟡";
pub const STATUS_ONLINE: &str = "🟢";
pub const STATUS_OFFLINE: &str = "🔴";
pub const STATUS_CACHED: &str = "🟠";
pub const NO_VALUE: &str = "—";

pub fn menu_bar_status(
    availability: RealmAvailability,
    syncing: bool,
    count: Option<i64>,
) -> String {
    if syncing {
        return t("menubar.a11y.syncing", "Syncing");
    }
    match availability {
        RealmAvailability::Healthy => player_status("Online", count.unwrap_or(0)),
        RealmAvailability::Unreachable if count.is_some() => {
            player_status("Cached", count.unwrap_or(0))
        }
        RealmAvailability::Unreachable => t("menubar.a11y.unavailable", "Unavailable"),
        RealmAvailability::Loading | RealmAvailability::ServerReportedDown => {
            t("menubar.a11y.offline", "Offline")
        }
    }
}

fn player_status(prefix: &str, count: i64) -> String {
    format!(
        "{prefix}, {count} {}",
        if count == 1 { "player" } else { "players" }
    )
}

fn signed_percent_words(value: f64) -> String {
    let magnitude = crate::crypto_format::one_decimal(value.abs());
    if value >= 0.0 {
        format!("up {magnitude} percent over 24 hours")
    } else {
        format!("down {magnitude} percent over 24 hours")
    }
}

pub fn menu_bar_status_and_change(status: &str, change: f64) -> String {
    format!("{status}. WOC {}.", signed_percent_words(change))
}

pub fn menu_bar_token_accessibility(price: &str, change: Option<f64>) -> String {
    let change = change.map_or_else(|| "24-hour change unavailable".into(), signed_percent_words);
    format!("WOC spot price {price} dollars, {change}.")
}

pub fn menu_bar_full_accessibility(status: &str, price: &str, change: Option<f64>) -> String {
    format!(
        "World of ClaudeCraft. {status}. {}",
        menu_bar_token_accessibility(price, change)
    )
}

pub fn market_quote_accessibility(price: &str, change: Option<f64>, cached: bool) -> String {
    let quote = menu_bar_token_accessibility(price, change);
    if cached {
        format!("Cached. {quote}")
    } else {
        quote
    }
}

pub fn chart_accessibility(latest: i64, peak: i64) -> String {
    format!("Players over time. Latest {latest}, peak {peak}.")
}

pub fn crypto_chart_accessibility(latest: f64, high: f64, low: f64, up: bool) -> String {
    format!(
        "$WOC price over time, trending {}. Latest {}, high {}, low {}.",
        if up { "up" } else { "down" },
        chart_price(latest),
        chart_price(high),
        chart_price(low)
    )
}

pub fn candle_close(interval: &str, price: &str) -> String {
    format!("{interval} close {price}")
}
pub fn percentage_accessibility(value: i64) -> String {
    format!("{value} percent")
}
pub fn player_count_accessibility(count: i64) -> String {
    format!("{count} {}", if count == 1 { "player" } else { "players" })
}
pub fn player_point_accessibility(count: i64, date: &str) -> String {
    format!("{}, {date}", player_count_accessibility(count))
}
pub fn player_series_accessibility(segment: Option<i64>) -> String {
    segment.map_or_else(
        || "Players".into(),
        |n| format!("Players, observed segment {n}"),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn candle_point_accessibility(
    date: &str,
    open: &str,
    high: &str,
    low: &str,
    close: &str,
    is_up: bool,
    change: &str,
) -> String {
    format!(
        "{date}; open {open}, high {high}, low {low}, close {close}, {} {change} percent",
        if is_up { "up" } else { "down" }
    )
}

pub fn app_version(version: &str, build: &str) -> String {
    format!("Version {version} · Build {build}")
}
pub fn market_alert_summary(threshold: i64, window: &str) -> String {
    format!("{threshold}% · {window}")
}
pub fn market_alert_accessibility(threshold: i64, window: &str) -> String {
    format!("Market alerts, {threshold} percent over {window}")
}
pub const CONTEXTUAL_ALERT_BLOCKED_VALUE: &str = "On, but blocked by macOS";
pub const CONTEXTUAL_ALERT_PERMISSION_DENIED_HELP: &str =
    "Notifications are blocked in macOS Settings. This alert remains enabled.";
pub const CONTEXTUAL_ALERT_PERMISSION_NEEDED_VALUE: &str = "On, permission needed";
pub const CONTEXTUAL_ALERT_PERMISSION_CHECKING_VALUE: &str = "On, checking permission";
pub const RELEASE_ALERT_LABEL: &str = "Release alerts";

pub fn alert_status_up(realm: &str, count: i64) -> (String, String) {
    let title = format!("{} {realm} is back", t("alert.status.up.title", "✅"));
    let body = if count == 1 {
        t("alert.status.up.body.one", "1 player online now.")
    } else {
        format!("{count} players online now.")
    };
    (title, body)
}

pub fn alert_status_down(realm: &str) -> (String, String) {
    (
        format!("{} {realm} looks down", t("alert.status.down.title", "⚠️")),
        t(
            "alert.status.down.body",
            "0 players online or the realm is unreachable.",
        ),
    )
}

pub fn alert_peak(realm: &str, count: i64) -> (String, String) {
    (
        format!("{} New peak on {realm}!", t("alert.peak.title", "🏆")),
        format!("{count} players online — a new record."),
    )
}

pub fn alert_crypto(pump: bool, percent: i64, price: &str) -> (String, String) {
    if pump {
        (
            t("alert.crypto.pump.title", "🚀 $WOC is Pumping!"),
            format!("The price just surged {percent}% to ${price}!"),
        )
    } else {
        (
            t("alert.crypto.dump.title", "📉 $WOC is Down"),
            format!("The price dropped {percent}% to ${price}."),
        )
    }
}

pub fn alert_population(above: bool, count: i64, threshold: i64) -> (String, String) {
    let title = if above {
        t(
            "alert.advanced.population.above.title",
            "🌍 The realm is bustling",
        )
    } else {
        t(
            "alert.advanced.population.below.title",
            "🌙 The realm is quiet",
        )
    };
    let subject = if count == 1 {
        "1 player is".into()
    } else {
        format!("{count} players are")
    };
    let body = if above {
        format!("{subject} online — your {threshold}-player alert was reached.")
    } else {
        format!("{subject} online — below your {threshold}-player alert.")
    };
    (title, body)
}

pub fn alert_price(above: bool, price: &str, target: &str) -> (String, String) {
    (
        t("alert.advanced.price.title", "🎯 $WOC target reached"),
        format!(
            "$WOC is {price}, at or {} your {target} target.",
            if above { "above" } else { "below" }
        ),
    )
}

pub fn alert_change(
    gain: bool,
    adjective: &str,
    duration: &str,
    change: f64,
    price: &str,
    threshold: f64,
) -> (String, String) {
    let (glyph, direction, comparison) = if gain {
        ("🚀", "Up", "above")
    } else {
        ("📉", "Down", "beyond")
    };
    (
        format!("{glyph} $WOC {adjective} move"),
        format!(
            "{direction} {:.1}% over {duration} at {price} — {comparison} your {:.1}% alert.",
            change.abs(),
            threshold
        ),
    )
}

pub fn alert_release(version: Option<&str>, body: Option<&str>) -> (String, String) {
    let version = version.unwrap_or("new release");
    (
        format!("✨ WoC {version} is here"),
        body.unwrap_or("A new World of ClaudeCraft release is available.")
            .to_owned(),
    )
}

pub fn alert_window(window: &str) -> (&'static str, &'static str) {
    match window {
        "1h" => ("1-hour", "1 hour"),
        "6h" => ("6-hour", "6 hours"),
        _ => ("24-hour", "24 hours"),
    }
}

/// Deterministic relative-update copy. Inputs are Unix seconds so callers can inject one clock.
pub fn relative_updated(date: Option<i64>, now: i64) -> String {
    let Some(date) = date else {
        return "never".into();
    };
    let seconds = now - date;
    if seconds < 2 {
        "updated just now".into()
    } else if seconds < 60 {
        format!("updated {seconds}s ago")
    } else if seconds / 60 < 60 {
        format!("updated {}m ago", seconds / 60)
    } else {
        format!("updated {}h ago", seconds / 3_600)
    }
}

pub fn relative_date(date: i64, now: i64) -> String {
    let seconds = now - date;
    if seconds >= 3_600 {
        format!("{} hr. ago", seconds / 3_600)
    } else if seconds >= 60 {
        format!("{} min. ago", seconds / 60)
    } else {
        format!("{seconds} sec. ago")
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod formatting_tests {
    use super::*;

    #[test]
    fn accessibility_text_matches_swift_formatting_tests() {
        let up = crypto_chart_accessibility(0.0005594, 0.0006, 0.0005, true);
        assert!(
            up.contains("trending up")
                && up.contains("0.0005594")
                && up.contains("high")
                && up.contains("low")
        );
        assert!(crypto_chart_accessibility(0.0005, 0.0006, 0.0004, false).contains("trending down"));
        let players = chart_accessibility(73, 120);
        assert!(players.contains("73") && players.contains("120"));
        assert_eq!(app_version("1.2.3", "45"), "Version 1.2.3 · Build 45");
        assert_eq!(market_alert_summary(12, "6h"), "12% · 6h");
        assert_eq!(
            market_alert_accessibility(12, "6h"),
            "Market alerts, 12 percent over 6h"
        );
        assert_eq!(CONTEXTUAL_ALERT_BLOCKED_VALUE, "On, but blocked by macOS");
        assert!(CONTEXTUAL_ALERT_PERMISSION_DENIED_HELP.contains("remains enabled"));
        assert_eq!(
            CONTEXTUAL_ALERT_PERMISSION_NEEDED_VALUE,
            "On, permission needed"
        );
        assert_eq!(
            CONTEXTUAL_ALERT_PERMISSION_CHECKING_VALUE,
            "On, checking permission"
        );
        assert_eq!(RELEASE_ALERT_LABEL, "Release alerts");
        assert_eq!(candle_close("5m", "$0.0005594"), "5m close $0.0005594");
        assert_eq!(percentage_accessibility(12), "12 percent");
        assert_eq!(player_count_accessibility(83), "83 players");
        assert_eq!(player_count_accessibility(1), "1 player");
        assert_eq!(
            player_point_accessibility(83, "Jul 10 at 14:30"),
            "83 players, Jul 10 at 14:30"
        );
        assert_eq!(
            player_point_accessibility(1, "Jul 10 at 14:30"),
            "1 player, Jul 10 at 14:30"
        );
        assert_eq!(player_series_accessibility(None), "Players");
        assert_eq!(
            player_series_accessibility(Some(2)),
            "Players, observed segment 2"
        );
        assert_eq!(candle_point_accessibility("Jul 10 at 14:30", "0.0005500", "0.0005700", "0.0005400", "0.0005594", true, "1.7"), "Jul 10 at 14:30; open 0.0005500, high 0.0005700, low 0.0005400, close 0.0005594, up 1.7 percent");
        assert!(market_quote_accessibility("0.5", None, true).starts_with("Cached. WOC spot price"));
    }

    #[test]
    fn relative_updated_formats_every_swift_boundary() {
        let base = 1_700_000_000;
        assert_eq!(relative_updated(None, base), "never");
        assert_eq!(relative_updated(Some(base), base + 1), "updated just now");
        assert_eq!(relative_updated(Some(base), base + 45), "updated 45s ago");
        assert_eq!(relative_updated(Some(base), base + 120), "updated 2m ago");
        assert_eq!(relative_updated(Some(base), base + 7_200), "updated 2h ago");
        let text = relative_date(base - 7_200, base);
        assert!(text.contains('2') && !text.starts_with("in "));
    }
}

/// Compact duration labels for poll and chart intervals.
/// Swift parity: AppText.compactDuration(seconds:) in Strings/AppText.swift.
pub fn compact_duration(seconds: i64) -> String {
    match seconds {
        10 => t("duration.compact.10s", "10s"),
        30 => t("duration.compact.30s", "30s"),
        60 => t("duration.compact.1m", "1m"),
        300 => t("duration.compact.5m", "5m"),
        900 => t("duration.compact.15m", "15m"),
        1_800 => t("duration.compact.30m", "30m"),
        3_600 => t("duration.compact.1h", "1h"),
        14_400 => t("duration.compact.4h", "4h"),
        21_600 => t("duration.compact.6h", "6h"),
        86_400 => t("duration.compact.24h", "24h"),
        604_800 => t("duration.compact.7d", "7d"),
        other => other.to_string(),
    }
}

/// "No internet connection". Swift parity: FetchError.friendlyMessage key "error.noInternet".
pub fn error_no_internet() -> String {
    t("error.noInternet", "No internet connection")
}

/// "Server timed out". Swift parity: FetchError.friendlyMessage key "error.timedOut".
pub fn error_timed_out() -> String {
    t("error.timedOut", "Server timed out")
}

/// "Can't reach server". Swift parity: FetchError.friendlyMessage key "error.unreachable".
pub fn error_unreachable() -> String {
    t("error.unreachable", "Can't reach server")
}

/// "Connection error". Swift parity: FetchError.friendlyMessage key "error.connection".
pub fn error_connection() -> String {
    t("error.connection", "Connection error")
}

/// "Couldn't load status". Swift parity: FetchError.friendlyMessage key "error.generic";
/// also the store-side fallback used by friendly(_:).
pub fn error_generic() -> String {
    t("error.generic", "Couldn't load status")
}
