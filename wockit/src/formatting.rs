//! Pure panel-label and crypto formatting.

use crate::{crypto_format, models::MenuBarDisplayMode, strings};

pub use crypto_format::{chart_price, price as crypto_price, signed_change};

pub use strings::{NO_VALUE, STATUS_CACHED, STATUS_LOADING, STATUS_OFFLINE, STATUS_ONLINE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Loading,
    Ok,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealmAvailability {
    Loading,
    Healthy,
    ServerReportedDown,
    Unreachable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFeedState {
    Idle,
    Loading,
    Live,
    Cached,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuBarPresentation {
    pub label: String,
    pub accessibility_label: String,
}

#[allow(clippy::too_many_arguments)]
pub fn menu_bar_presentation(
    mode: MenuBarDisplayMode,
    count: Option<i64>,
    availability: RealmAvailability,
    phase: Phase,
    is_online: bool,
    price: Option<&str>,
    change_24h: Option<f64>,
    price_state: DataFeedState,
) -> MenuBarPresentation {
    let glyph = status_glyph(availability, phase, is_online, count.is_some());
    let players = format!(
        "{glyph} {}",
        count.map_or_else(|| NO_VALUE.into(), |n| n.to_string())
    );
    let status = strings::menu_bar_status(
        availability,
        availability == RealmAvailability::Loading && phase == Phase::Loading,
        count,
    );
    let fallback = || MenuBarPresentation {
        label: players.clone(),
        accessibility_label: status.clone(),
    };

    if mode == MenuBarDisplayMode::IconOnly {
        return MenuBarPresentation {
            label: String::new(),
            accessibility_label: status,
        };
    }
    let fresh_price = (price_state == DataFeedState::Live)
        .then_some(price)
        .flatten();
    match mode {
        MenuBarDisplayMode::Players => fallback(),
        MenuBarDisplayMode::PlayersAndChange => match (fresh_price, change_24h) {
            (Some(_), Some(change)) => MenuBarPresentation {
                label: format!("{players} · WOC {}", signed_change(change)),
                accessibility_label: strings::menu_bar_status_and_change(&status, change),
            },
            _ => fallback(),
        },
        MenuBarDisplayMode::Token => match fresh_price {
            Some(raw) => MenuBarPresentation {
                label: change_24h.map_or_else(
                    || crypto_price(raw),
                    |c| format!("{} {}", crypto_price(raw), signed_change(c)),
                ),
                accessibility_label: strings::menu_bar_token_accessibility(raw, change_24h),
            },
            None => fallback(),
        },
        MenuBarDisplayMode::Full => match fresh_price {
            Some(raw) => {
                let crypto = change_24h.map_or_else(
                    || crypto_price(raw),
                    |c| format!("{} ({})", crypto_price(raw), signed_change(c)),
                );
                MenuBarPresentation {
                    label: format!("{crypto} {players}"),
                    accessibility_label: strings::menu_bar_full_accessibility(
                        &status, raw, change_24h,
                    ),
                }
            }
            None => fallback(),
        },
        MenuBarDisplayMode::IconOnly => unreachable!(),
    }
}

fn status_glyph(
    availability: RealmAvailability,
    phase: Phase,
    is_online: bool,
    has_count: bool,
) -> &'static str {
    if is_online {
        return STATUS_ONLINE;
    }
    match availability {
        RealmAvailability::Loading if phase == Phase::Loading => STATUS_LOADING,
        RealmAvailability::Loading | RealmAvailability::ServerReportedDown => STATUS_OFFLINE,
        RealmAvailability::Healthy => STATUS_ONLINE,
        RealmAvailability::Unreachable if has_count => STATUS_CACHED,
        RealmAvailability::Unreachable => STATUS_OFFLINE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presentation(
        mode: MenuBarDisplayMode,
        count: Option<i64>,
        availability: RealmAvailability,
        price: Option<&str>,
        change: Option<f64>,
        state: DataFeedState,
    ) -> MenuBarPresentation {
        menu_bar_presentation(
            mode,
            count,
            availability,
            Phase::Ok,
            availability == RealmAvailability::Healthy,
            price,
            change,
            state,
        )
    }

    #[test]
    fn chart_price_keeps_fixed_point_and_four_significant_figures() {
        assert_eq!(chart_price(0.0005594), "0.0005594");
        assert_eq!(chart_price(0.00005594), "0.00005594");
        assert!(!chart_price(0.00005594).to_lowercase().contains('e'));
        assert_eq!(chart_price(1.5), "1.500");
        assert_eq!(chart_price(0.0), "0");
        assert_eq!(chart_price(-1.5), "-1.5");
        assert_eq!(chart_price(f64::NAN), "nan");
        assert_eq!(chart_price(f64::NEG_INFINITY), "-inf");
    }

    #[test]
    fn price_and_signed_change_fragments_are_stable() {
        assert_eq!(crypto_price("0.0005594"), "$0.0005594");
        assert_eq!(signed_change(47.59), "+47.6%");
        assert_eq!(signed_change(-3.0), "-3.0%");
        assert_eq!(signed_change(f64::NAN), "nan%");
    }

    #[test]
    fn visible_and_spoken_output_follow_each_selected_mode() {
        let players = presentation(
            MenuBarDisplayMode::Players,
            Some(7),
            RealmAvailability::Healthy,
            Some("0.0005594"),
            Some(47.59),
            DataFeedState::Live,
        );
        assert_eq!(players.label, "🟢 7");
        assert_eq!(players.accessibility_label, "Online, 7 players");
        let change = presentation(
            MenuBarDisplayMode::PlayersAndChange,
            Some(7),
            RealmAvailability::Healthy,
            Some("0.0005594"),
            Some(47.59),
            DataFeedState::Live,
        );
        assert_eq!(change.label, "🟢 7 · WOC +47.6%");
        assert!(change.accessibility_label.contains("WOC up 47.6 percent"));
        let token = presentation(
            MenuBarDisplayMode::Token,
            Some(7),
            RealmAvailability::Healthy,
            Some("0.0005594"),
            Some(47.59),
            DataFeedState::Live,
        );
        assert_eq!(token.label, "$0.0005594 +47.6%");
        assert!(token.accessibility_label.starts_with("WOC spot price"));
        assert!(!token.accessibility_label.contains("Online"));
        let full = presentation(
            MenuBarDisplayMode::Full,
            Some(7),
            RealmAvailability::Healthy,
            Some("0.0005594"),
            Some(47.59),
            DataFeedState::Live,
        );
        assert_eq!(full.label, "$0.0005594 (+47.6%) 🟢 7");
        assert!(full.accessibility_label.contains("Online, 7 players"));
        assert!(full.accessibility_label.contains("WOC spot price"));
    }

    #[test]
    fn stale_price_falls_back_in_every_price_bearing_mode() {
        for mode in [
            MenuBarDisplayMode::PlayersAndChange,
            MenuBarDisplayMode::Token,
            MenuBarDisplayMode::Full,
        ] {
            let value = presentation(
                mode,
                Some(1),
                RealmAvailability::Unreachable,
                Some("0.0005594"),
                Some(47.59),
                DataFeedState::Cached,
            );
            assert_eq!(value.label, "🟠 1");
            assert_eq!(value.accessibility_label, "Cached, 1 player");
        }
        let unavailable = presentation(
            MenuBarDisplayMode::Token,
            None,
            RealmAvailability::Unreachable,
            None,
            None,
            DataFeedState::Unavailable,
        );
        assert_eq!(unavailable.label, "🔴 —");
        assert_eq!(unavailable.accessibility_label, "Unavailable");
    }

    #[test]
    fn icon_only_has_empty_visible_label_and_semantic_name() {
        let value = presentation(
            MenuBarDisplayMode::IconOnly,
            Some(7),
            RealmAvailability::Healthy,
            Some("0.5"),
            Some(1.0),
            DataFeedState::Live,
        );
        assert_eq!(value.label, "");
        assert_eq!(value.accessibility_label, "Online, 7 players");
    }
}
