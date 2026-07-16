//! COSMIC session and panel-configuration detection for tray policy.

use cosmic_config::{Config, ConfigGet};

pub const APPLET_ID: &str = "io.github.fernandox7.wocplayercount.cosmic-applet";
const PANEL_CONFIG_ID: &str = "com.system76.CosmicPanel";
const PANEL_CONFIG_VERSION: u64 = 1;
pub type PluginWings = (Vec<String>, Vec<String>);

pub trait PanelConfiguration {
    fn entries(&self) -> Result<Vec<String>, String>;
    fn plugins_center(&self, entry: &str) -> Result<Option<Vec<String>>, String>;
    fn plugins_wings(&self, entry: &str) -> Result<Option<PluginWings>, String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CosmicConfig;

impl CosmicConfig {
    fn config(id: &str) -> Result<Config, String> {
        Config::new(id, PANEL_CONFIG_VERSION).map_err(|error| error.to_string())
    }

    fn entry_config(entry: &str) -> Result<Config, String> {
        Self::config(&format!("{PANEL_CONFIG_ID}.{entry}"))
    }
}

impl PanelConfiguration for CosmicConfig {
    fn entries(&self) -> Result<Vec<String>, String> {
        Self::config(PANEL_CONFIG_ID)?
            .get("entries")
            .map_err(|error| error.to_string())
    }

    fn plugins_center(&self, entry: &str) -> Result<Option<Vec<String>>, String> {
        Self::entry_config(entry)?
            .get("plugins_center")
            .map_err(|error| error.to_string())
    }

    fn plugins_wings(&self, entry: &str) -> Result<Option<PluginWings>, String> {
        Self::entry_config(entry)?
            .get("plugins_wings")
            .map_err(|error| error.to_string())
    }
}

pub fn is_cosmic_session<I, S>(desktop_values: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    desktop_values.into_iter().any(|value| {
        value
            .as_ref()
            .split(':')
            .any(|desktop| desktop.eq_ignore_ascii_case("cosmic"))
    })
}

pub fn current_session_is_cosmic() -> bool {
    is_cosmic_session(
        [
            "XDG_CURRENT_DESKTOP",
            "XDG_SESSION_DESKTOP",
            "DESKTOP_SESSION",
        ]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok()),
    )
}

/// Returns an error rather than guessing when any active profile cannot be read completely.
/// Callers deliberately retain the generic tray on error.
pub fn native_applet_configured(
    session_is_cosmic: bool,
    config: &impl PanelConfiguration,
) -> Result<bool, String> {
    if !session_is_cosmic {
        return Ok(false);
    }

    for entry in config.entries()? {
        let center = config.plugins_center(&entry)?.unwrap_or_default();
        let (left, right) = config
            .plugins_wings(&entry)?
            .unwrap_or_else(|| (Vec::new(), Vec::new()));
        if center
            .iter()
            .chain(left.iter())
            .chain(right.iter())
            .any(|plugin| plugin == APPLET_ID)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn should_show_tray(session_is_cosmic: bool, config: &impl PanelConfiguration) -> bool {
    !native_applet_configured(session_is_cosmic, config).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeConfig {
        entries: Result<Vec<String>, String>,
        center: HashMap<String, Result<Option<Vec<String>>, String>>,
        wings: HashMap<String, Result<Option<PluginWings>, String>>,
    }

    impl Default for FakeConfig {
        fn default() -> Self {
            Self {
                entries: Ok(Vec::new()),
                center: HashMap::new(),
                wings: HashMap::new(),
            }
        }
    }

    impl PanelConfiguration for FakeConfig {
        fn entries(&self) -> Result<Vec<String>, String> {
            self.entries.clone()
        }

        fn plugins_center(&self, entry: &str) -> Result<Option<Vec<String>>, String> {
            self.center.get(entry).cloned().unwrap_or(Ok(None))
        }

        fn plugins_wings(&self, entry: &str) -> Result<Option<PluginWings>, String> {
            self.wings.get(entry).cloned().unwrap_or(Ok(None))
        }
    }

    fn config_with_entries(entries: &[&str]) -> FakeConfig {
        FakeConfig {
            entries: Ok(entries.iter().map(|value| (*value).to_owned()).collect()),
            ..FakeConfig::default()
        }
    }

    #[test]
    fn non_cosmic_session_always_keeps_tray() {
        let mut config = config_with_entries(&["Panel"]);
        config
            .center
            .insert("Panel".into(), Ok(Some(vec![APPLET_ID.to_owned()])));
        assert!(should_show_tray(false, &config));
        assert!(is_cosmic_session(["GNOME:COSMIC"]));
        assert!(!is_cosmic_session(["GNOME", "KDE"]));
    }

    #[test]
    fn cosmic_applet_in_any_panel_position_suppresses_tray() {
        let mut center = config_with_entries(&["Panel", "Dock"]);
        center
            .center
            .insert("Dock".into(), Ok(Some(vec![APPLET_ID.to_owned()])));
        assert!(!should_show_tray(true, &center));

        let mut wing = config_with_entries(&["Panel"]);
        wing.wings.insert(
            "Panel".into(),
            Ok(Some((Vec::new(), vec![APPLET_ID.to_owned()]))),
        );
        assert!(!should_show_tray(true, &wing));
    }

    #[test]
    fn cosmic_without_applet_retains_tray_fallback() {
        let mut config = config_with_entries(&["Panel", "Dock"]);
        config.center.insert(
            "Panel".into(),
            Ok(Some(vec!["com.system76.CosmicAppletTime".into()])),
        );
        assert!(should_show_tray(true, &config));
    }

    #[test]
    fn unreadable_or_malformed_configuration_fails_safe_to_tray() {
        let unreadable = FakeConfig {
            entries: Err("permission denied".into()),
            ..FakeConfig::default()
        };
        assert!(should_show_tray(true, &unreadable));

        let mut malformed_profile = config_with_entries(&["Panel"]);
        malformed_profile
            .wings
            .insert("Panel".into(), Err("invalid RON".into()));
        assert!(should_show_tray(true, &malformed_profile));
    }

    #[test]
    fn configuration_changes_reverse_policy_without_starting_an_app() {
        let absent = config_with_entries(&["Panel"]);
        assert!(should_show_tray(true, &absent));

        let mut present = config_with_entries(&["Panel"]);
        present
            .center
            .insert("Panel".into(), Ok(Some(vec![APPLET_ID.to_owned()])));
        assert!(!should_show_tray(true, &present));
        assert!(should_show_tray(true, &absent));
    }
}
