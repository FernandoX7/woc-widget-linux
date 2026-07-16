//! Persisted menu-bar display modes.

use serde::{Deserialize, Serialize};

/// Controls how much information is reserved in the panel label.
///
/// The first four raw values are frozen by the macOS app. `iconOnly` is the additive
/// Linux fallback for panels which do not render StatusNotifierItem titles.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MenuBarDisplayMode {
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

impl MenuBarDisplayMode {
    pub const ALL_CASES: [Self; 5] = [
        Self::Players,
        Self::PlayersAndChange,
        Self::Token,
        Self::Full,
        Self::IconOnly,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_raw_values_are_frozen_and_players_change_is_default() {
        let cases = [
            (MenuBarDisplayMode::Players, "\"players\""),
            (MenuBarDisplayMode::PlayersAndChange, "\"playersAndChange\""),
            (MenuBarDisplayMode::Token, "\"token\""),
            (MenuBarDisplayMode::Full, "\"full\""),
            (MenuBarDisplayMode::IconOnly, "\"iconOnly\""),
        ];
        for (mode, raw) in cases {
            assert_eq!(serde_json::to_string(&mode).unwrap(), raw);
            assert_eq!(
                serde_json::from_str::<MenuBarDisplayMode>(raw).unwrap(),
                mode
            );
        }
        assert_eq!(
            MenuBarDisplayMode::default(),
            MenuBarDisplayMode::PlayersAndChange
        );
    }

    #[test]
    fn corrupt_raw_values_do_not_silently_become_a_mode() {
        assert!(serde_json::from_str::<MenuBarDisplayMode>("\"Players\"").is_err());
        assert!(serde_json::from_str::<MenuBarDisplayMode>("\"corrupt\"").is_err());
    }
}
