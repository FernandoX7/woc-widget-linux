use std::time::{Duration, Instant};

use wockit::{
    config,
    formatting::{self, DataFeedState, MenuBarPresentation, Phase, RealmAvailability},
    models::{CryptoQuote, MenuBarDisplayMode, StatusResponse},
};

#[derive(Debug)]
pub struct RuntimeState {
    status: Option<StatusResponse>,
    quote: Option<CryptoQuote>,
    quote_success: Option<Instant>,
    quote_state: DataFeedState,
    status_loading: bool,
    status_unreachable: bool,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            status: None,
            quote: None,
            quote_success: None,
            quote_state: DataFeedState::Unavailable,
            status_loading: false,
            status_unreachable: false,
        }
    }
}

impl RuntimeState {
    pub fn status_started(&mut self) {
        self.status_loading = self.status.is_none();
    }

    pub fn status_finished(&mut self, result: Result<StatusResponse, wockit::error::FetchError>) {
        self.status_loading = false;
        match result {
            Ok(status) => {
                self.status = Some(status);
                self.status_unreachable = false;
            }
            Err(_) => self.status_unreachable = true,
        }
    }

    pub fn quote_finished(&mut self, result: Result<CryptoQuote, wockit::error::FetchError>) {
        match result {
            Ok(quote) if valid_quote(&quote) => {
                self.quote = Some(quote);
                self.quote_success = Some(Instant::now());
                self.quote_state = DataFeedState::Live;
            }
            Ok(_) | Err(_) if self.quote.is_some() => self.quote_state = DataFeedState::Cached,
            Ok(_) | Err(_) => self.quote_state = DataFeedState::Unavailable,
        }
    }

    pub fn presentation(
        &self,
        mode: MenuBarDisplayMode,
        market_interval: Duration,
        now: Instant,
    ) -> MenuBarPresentation {
        let availability = if self.status_unreachable {
            RealmAvailability::Unreachable
        } else if let Some(status) = &self.status {
            if status.ok {
                RealmAvailability::Healthy
            } else {
                RealmAvailability::ServerReportedDown
            }
        } else {
            RealmAvailability::Loading
        };
        let price_state =
            price_feed_state(self.quote_success, self.quote_state, market_interval, now);
        formatting::menu_bar_presentation(
            mode,
            self.status.as_ref().map(|status| status.players_online),
            availability,
            if self.status_loading {
                Phase::Loading
            } else {
                Phase::Ok
            },
            self.status.as_ref().is_some_and(|status| status.ok),
            self.quote.as_ref().map(|quote| quote.price.as_str()),
            self.quote.as_ref().map(|quote| quote.change24h),
            price_state,
        )
    }
}

fn valid_quote(quote: &CryptoQuote) -> bool {
    quote
        .price
        .parse::<f64>()
        .is_ok_and(|price| price.is_finite() && price > 0.0)
        && quote.change24h.is_finite()
}

fn price_feed_state(
    success: Option<Instant>,
    state: DataFeedState,
    market_interval: Duration,
    now: Instant,
) -> DataFeedState {
    if state == DataFeedState::Live
        && success.is_some_and(|success| {
            now.saturating_duration_since(success) < config::panel_price_freshness(market_interval)
        })
    {
        DataFeedState::Live
    } else if success.is_some() {
        DataFeedState::Cached
    } else {
        DataFeedState::Unavailable
    }
}

pub fn reserved_width(mode: MenuBarDisplayMode) -> f32 {
    // Precomputed with the current COSMIC interface font using the longest representative
    // formatter output for each mode, then rounded up with padding. Keeping these per-mode
    // maxima fixed follows minimon's anti-wobble pattern while the live text changes.
    match mode {
        MenuBarDisplayMode::Players => 72.0,
        MenuBarDisplayMode::PlayersAndChange => 166.0,
        MenuBarDisplayMode::Token => 142.0,
        MenuBarDisplayMode::Full => 220.0,
        MenuBarDisplayMode::IconOnly => 32.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_mode_has_a_stable_distinct_reservation() {
        let widths: Vec<_> = MenuBarDisplayMode::ALL_CASES
            .into_iter()
            .map(reserved_width)
            .collect();
        assert!(widths.windows(2).all(|pair| pair[0] != pair[1]));
        assert_eq!(widths[3], 220.0);
    }

    #[test]
    fn formatter_owns_loading_and_glyph_output() {
        let mut state = RuntimeState::default();
        state.status_started();
        let label = state
            .presentation(
                MenuBarDisplayMode::Players,
                Duration::from_secs(60),
                Instant::now(),
            )
            .label;
        assert_eq!(label, format!("{} —", formatting::STATUS_LOADING));
    }

    #[test]
    fn every_crypto_cadence_is_live_before_and_cached_at_its_exact_deadline() {
        let success = Instant::now();
        for interval in [30, 60, 300, 900] {
            let interval = Duration::from_secs(interval);
            let deadline = config::panel_price_freshness(interval);
            assert_eq!(
                price_feed_state(
                    Some(success),
                    DataFeedState::Live,
                    interval,
                    success + deadline - Duration::from_nanos(1),
                ),
                DataFeedState::Live
            );
            assert_eq!(
                price_feed_state(
                    Some(success),
                    DataFeedState::Live,
                    interval,
                    success + deadline,
                ),
                DataFeedState::Cached
            );
        }
    }

    #[test]
    fn one_minute_quote_has_no_twenty_to_sixty_second_dropout() {
        let success = Instant::now();
        for age in [20, 59, 60] {
            assert_eq!(
                price_feed_state(
                    Some(success),
                    DataFeedState::Live,
                    Duration::from_secs(60),
                    success + Duration::from_secs(age),
                ),
                DataFeedState::Live
            );
        }
    }

    #[test]
    fn shorter_cadence_keeps_the_exact_twenty_second_floor_boundary() {
        let success = Instant::now();
        assert_eq!(
            price_feed_state(
                Some(success),
                DataFeedState::Live,
                Duration::from_secs(1),
                success + Duration::from_secs(20) - Duration::from_nanos(1),
            ),
            DataFeedState::Live
        );
        assert_eq!(
            price_feed_state(
                Some(success),
                DataFeedState::Live,
                Duration::from_secs(1),
                success + Duration::from_secs(20),
            ),
            DataFeedState::Cached
        );
    }

    #[test]
    fn failed_quote_refresh_immediately_marks_the_value_cached() {
        let mut state = RuntimeState {
            status: Some(StatusResponse::new(true, None, 7, None)),
            quote: Some(CryptoQuote {
                price: "0.0005594".into(),
                change24h: 47.59,
                market: std::collections::HashMap::new(),
                liquidity_usd: None,
                fdv_usd: None,
                market_cap_usd: None,
                pair_url: None,
            }),
            quote_success: Some(Instant::now()),
            quote_state: DataFeedState::Live,
            ..RuntimeState::default()
        };
        state.quote_finished(Err(wockit::error::FetchError::Decode));
        for mode in [MenuBarDisplayMode::Token, MenuBarDisplayMode::Full] {
            let expected = formatting::menu_bar_presentation(
                mode,
                Some(7),
                RealmAvailability::Healthy,
                Phase::Ok,
                true,
                state.quote.as_ref().map(|quote| quote.price.as_str()),
                state.quote.as_ref().map(|quote| quote.change24h),
                DataFeedState::Cached,
            );
            assert_eq!(
                state.presentation(mode, Duration::from_secs(60), Instant::now()),
                expected
            );
            assert!(!expected.label.contains('$'));
            assert!(expected.label.contains('7'));
        }
    }

    #[test]
    fn stalled_refresh_cannot_keep_an_old_quote_live_forever() {
        let success = Instant::now();
        assert_eq!(
            price_feed_state(
                Some(success),
                DataFeedState::Live,
                Duration::from_secs(300),
                success + Duration::from_secs(341),
            ),
            DataFeedState::Live
        );
        assert_eq!(
            price_feed_state(
                Some(success),
                DataFeedState::Live,
                Duration::from_secs(300),
                success + Duration::from_secs(342),
            ),
            DataFeedState::Cached
        );
    }

    #[test]
    fn malformed_quote_never_becomes_live() {
        let mut state = RuntimeState::default();
        state.quote_finished(Ok(CryptoQuote {
            price: "nan".into(),
            change24h: 1.0,
            market: std::collections::HashMap::new(),
            liquidity_usd: None,
            fdv_usd: None,
            market_cap_usd: None,
            pair_url: None,
        }));
        assert_eq!(state.quote_state, DataFeedState::Unavailable);
    }

    #[test]
    fn every_mode_is_byte_identical_to_the_shared_formatter() {
        let now = Instant::now();
        let state = RuntimeState {
            status: Some(StatusResponse::new(true, None, 7, None)),
            quote: Some(CryptoQuote {
                price: "0.0005594".into(),
                change24h: 47.59,
                market: std::collections::HashMap::new(),
                liquidity_usd: None,
                fdv_usd: None,
                market_cap_usd: None,
                pair_url: None,
            }),
            quote_success: Some(now),
            quote_state: DataFeedState::Live,
            ..RuntimeState::default()
        };

        for mode in MenuBarDisplayMode::ALL_CASES {
            let expected = formatting::menu_bar_presentation(
                mode,
                Some(7),
                RealmAvailability::Healthy,
                Phase::Ok,
                true,
                Some("0.0005594"),
                Some(47.59),
                DataFeedState::Live,
            );
            assert_eq!(
                state.presentation(mode, Duration::from_secs(60), now),
                expected,
                "mode {mode:?}"
            );
        }
    }

    #[test]
    fn tray_and_applet_presentations_are_byte_identical_in_all_five_modes() {
        use woc_app::{
            poller::{
                DataFeedState as PollFeedState, Feed, PollerSnapshot,
                RealmAvailability as PollRealmAvailability,
            },
            settings::DisplayMode,
            tray,
        };

        let now = Instant::now();
        let status = StatusResponse::new(true, None, 7, None);
        let quote = CryptoQuote {
            price: "0.0005594".into(),
            change24h: 47.59,
            market: std::collections::HashMap::new(),
            liquidity_usd: None,
            fdv_usd: None,
            market_cap_usd: None,
            pair_url: None,
        };
        let applet = RuntimeState {
            status: Some(status.clone()),
            quote: Some(quote.clone()),
            quote_success: Some(now),
            quote_state: DataFeedState::Live,
            ..RuntimeState::default()
        };
        let tray_snapshot = PollerSnapshot {
            status: Feed {
                value: Some(status),
                state: PollFeedState::Live,
                ..Feed::default()
            },
            realm_availability: PollRealmAvailability::Healthy,
            quote: Feed {
                value: Some(quote),
                state: PollFeedState::Live,
                ..Feed::default()
            },
            ..PollerSnapshot::default()
        };
        let modes = [
            (MenuBarDisplayMode::Players, DisplayMode::Players),
            (
                MenuBarDisplayMode::PlayersAndChange,
                DisplayMode::PlayersAndChange,
            ),
            (MenuBarDisplayMode::Token, DisplayMode::Token),
            (MenuBarDisplayMode::Full, DisplayMode::Full),
            (MenuBarDisplayMode::IconOnly, DisplayMode::IconOnly),
        ];
        for (applet_mode, tray_mode) in modes {
            assert_eq!(
                applet.presentation(applet_mode, Duration::from_secs(60), now),
                tray::presentation(&tray_snapshot, tray_mode),
                "mode {applet_mode:?}"
            );
        }
    }
}
