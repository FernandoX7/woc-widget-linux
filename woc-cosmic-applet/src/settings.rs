use std::{env, io, path::PathBuf, time::Duration};

use serde::Deserialize;
use wockit::{
    config::{self, PollInterval},
    models::MenuBarDisplayMode,
};

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppletSettings {
    pub menu_bar_display_mode: MenuBarDisplayMode,
    pub poll_seconds: f64,
    pub crypto_poll_seconds: f64,
}

impl Default for AppletSettings {
    fn default() -> Self {
        Self {
            menu_bar_display_mode: MenuBarDisplayMode::default(),
            poll_seconds: config::poll::DEFAULT_PLAYER_SECONDS,
            crypto_poll_seconds: config::poll::DEFAULT_CRYPTO_SECONDS,
        }
    }
}

impl AppletSettings {
    pub fn normalized(mut self) -> Self {
        self.poll_seconds = PollInterval::normalize(
            self.poll_seconds,
            &PollInterval::player_options(),
            PollInterval::OneMinute,
        );
        self.crypto_poll_seconds = PollInterval::normalize(
            self.crypto_poll_seconds,
            &PollInterval::crypto_options(),
            PollInterval::FiveMinutes,
        );
        self
    }

    pub fn status_interval(self) -> Duration {
        Duration::from_secs_f64(self.poll_seconds)
    }

    pub fn quote_interval(self) -> Duration {
        Duration::from_secs_f64(self.crypto_poll_seconds)
    }
}

pub fn default_path() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path).join("woc-widget/settings.json"));
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".config/woc-widget/settings.json"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
}

pub async fn load(path: PathBuf) -> AppletSettings {
    tokio::fs::read(path)
        .await
        .ok()
        .and_then(|bytes| serde_json::from_slice::<AppletSettings>(&bytes).ok())
        .unwrap_or_default()
        .normalized()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_the_frozen_shared_keys_and_normalizes_cadences() {
        let settings: AppletSettings = serde_json::from_str(
            r#"{"menuBarDisplayMode":"full","pollSeconds":45,"cryptoPollSeconds":121,"alertsEnabled":false}"#,
        )
        .unwrap();
        let settings = settings.normalized();
        assert_eq!(settings.menu_bar_display_mode, MenuBarDisplayMode::Full);
        assert_eq!(settings.poll_seconds, 60.0);
        assert_eq!(settings.crypto_poll_seconds, 60.0);
    }

    #[test]
    fn corrupt_file_uses_the_same_defaults() {
        assert!(serde_json::from_str::<AppletSettings>("not json").is_err());
        assert_eq!(AppletSettings::default().poll_seconds, 60.0);
        assert_eq!(AppletSettings::default().crypto_poll_seconds, 60.0);
    }
}
