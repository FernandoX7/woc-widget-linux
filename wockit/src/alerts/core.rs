//! Pure, UI-free realm, local-record, and legacy crypto alert reducers.

use chrono::{DateTime, Utc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertKind {
    Status,
    Peak,
    CryptoPump,
    CryptoDump,
}

impl AlertKind {
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Peak => "peak",
            Self::CryptoPump => "crypto_pump",
            Self::CryptoDump => "crypto_dump",
        }
    }

    /// The caller supplies the time; constructing an ID never reads the wall clock.
    pub fn request_id(self, now: impl FnOnce() -> f64) -> String {
        let timestamp = now();
        let timestamp = if timestamp.fract() == 0.0 {
            format!("{timestamp:.1}")
        } else {
            timestamp.to_string()
        };
        format!("{}-{timestamp}", self.prefix())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlertSettings {
    pub status_enabled: bool,
    pub peak_enabled: bool,
    pub crypto_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlertDecision {
    StatusUp { realm: String, count: i64 },
    StatusDown { realm: String },
    Peak { realm: String, count: i64 },
    CryptoPump { percent: i64, price: String },
    CryptoDump { percent: i64, price: String },
}

impl AlertDecision {
    pub const fn kind(&self) -> AlertKind {
        match self {
            Self::StatusUp { .. } | Self::StatusDown { .. } => AlertKind::Status,
            Self::Peak { .. } => AlertKind::Peak,
            Self::CryptoPump { .. } => AlertKind::CryptoPump,
            Self::CryptoDump { .. } => AlertKind::CryptoDump,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusAlertState {
    pub confirmed_up: Option<bool>,
    pub consecutive_remote_failures: usize,
    pub recovery_notification_armed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusAlertObservation {
    Healthy,
    Failure { counts_toward_outage: bool },
}

pub fn evaluate_debounced_status(
    state: StatusAlertState,
    observation: StatusAlertObservation,
    required_failures: usize,
    realm: &str,
    count: i64,
    settings: AlertSettings,
) -> (StatusAlertState, Option<AlertDecision>) {
    match observation {
        StatusAlertObservation::Healthy => {
            let decision = (state.confirmed_up == Some(false)
                && state.recovery_notification_armed
                && settings.status_enabled)
                .then(|| AlertDecision::StatusUp {
                    realm: realm.to_owned(),
                    count,
                });
            (
                StatusAlertState {
                    confirmed_up: Some(true),
                    consecutive_remote_failures: 0,
                    recovery_notification_armed: false,
                },
                decision,
            )
        }
        StatusAlertObservation::Failure {
            counts_toward_outage: false,
        } => (
            StatusAlertState {
                consecutive_remote_failures: 0,
                ..state
            },
            None,
        ),
        StatusAlertObservation::Failure {
            counts_toward_outage: true,
        } => {
            let confirmation = required_failures.max(1);
            let failures = state
                .consecutive_remote_failures
                .saturating_add(1)
                .min(confirmation);
            if failures < confirmation {
                return (
                    StatusAlertState {
                        consecutive_remote_failures: failures,
                        ..state
                    },
                    None,
                );
            }
            let decision =
                (state.confirmed_up == Some(true) && settings.status_enabled).then(|| {
                    AlertDecision::StatusDown {
                        realm: realm.to_owned(),
                    }
                });
            (
                StatusAlertState {
                    confirmed_up: Some(false),
                    consecutive_remote_failures: failures,
                    recovery_notification_armed: decision.is_some(),
                },
                decision,
            )
        }
    }
}

pub fn evaluate_status_transition(
    previous_up: Option<bool>,
    up: bool,
    realm: &str,
    count: i64,
    settings: AlertSettings,
) -> (bool, Option<AlertDecision>) {
    let decision = match previous_up {
        Some(previous) if previous != up && settings.status_enabled => Some(if up {
            AlertDecision::StatusUp {
                realm: realm.to_owned(),
                count,
            }
        } else {
            AlertDecision::StatusDown {
                realm: realm.to_owned(),
            }
        }),
        _ => None,
    };
    (up, decision)
}

pub fn evaluate_peak(
    count: i64,
    at: DateTime<Utc>,
    current_peak: i64,
    current_peak_date: Option<DateTime<Utc>>,
    realm: &str,
    settings: AlertSettings,
) -> (i64, Option<DateTime<Utc>>, Option<AlertDecision>) {
    if count <= current_peak {
        return (current_peak, current_peak_date, None);
    }
    let decision = (current_peak > 0 && settings.peak_enabled).then(|| AlertDecision::Peak {
        realm: realm.to_owned(),
        count,
    });
    (count, Some(at), decision)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeakObservation {
    pub count: i64,
    pub at: DateTime<Utc>,
    pub realm: String,
}

/// Store-facing coordinator that defers observations until retained history is loaded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalRecordAlertState {
    history_loaded: bool,
    pending: Vec<PeakObservation>,
}

impl LocalRecordAlertState {
    pub fn observe(&mut self, observation: PeakObservation) -> Option<PeakObservation> {
        if self.history_loaded {
            Some(observation)
        } else {
            self.pending.push(observation);
            None
        }
    }

    /// Marks history loaded and returns queued observations in arrival order. The caller first
    /// reconciles its persisted peak with loaded history, then evaluates this returned batch.
    pub fn finish_history_load(&mut self) -> Vec<PeakObservation> {
        self.history_loaded = true;
        std::mem::take(&mut self.pending)
    }

    pub const fn history_loaded(&self) -> bool {
        self.history_loaded
    }
}

pub fn evaluate_crypto(
    current_price: f64,
    baseline: f64,
    threshold_percent: f64,
    settings: AlertSettings,
) -> (f64, Option<AlertDecision>) {
    if current_price <= 0.0 {
        return (baseline, None);
    }
    if baseline == 0.0 {
        return (current_price, None);
    }
    if !settings.crypto_enabled {
        return (baseline, None);
    }

    let change_ratio = current_price / baseline;
    let threshold_ratio = threshold_percent / 100.0;
    let price = swift_double_description(current_price);
    if change_ratio >= 1.0 + threshold_ratio {
        let percent = ((change_ratio - 1.0) * 100.0).round() as i64;
        (
            current_price,
            Some(AlertDecision::CryptoPump { percent, price }),
        )
    } else if change_ratio <= 1.0 - threshold_ratio {
        let percent = ((1.0 - change_ratio) * 100.0).round() as i64;
        (
            current_price,
            Some(AlertDecision::CryptoDump { percent, price }),
        )
    } else {
        (baseline, None)
    }
}

fn swift_double_description(value: f64) -> String {
    let rendered = value.to_string();
    if value.fract() == 0.0 && value.is_finite() {
        format!("{rendered}.0")
    } else {
        rendered
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;

    const ALL_ON: AlertSettings = AlertSettings {
        status_enabled: true,
        peak_enabled: true,
        crypto_enabled: true,
    };

    fn date() -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000, 0).unwrap()
    }

    // Swift source: AlertEngineTests.statusSkipsFirstObservationButAdvancesState
    #[test]
    fn status_skips_first_observation_but_advances_state() {
        assert_eq!(
            evaluate_status_transition(None, true, "R", 5, ALL_ON),
            (true, None)
        );
    }

    // Swift source: AlertEngineTests.statusNoAlertWhenUnchanged
    #[test]
    fn status_no_alert_when_unchanged() {
        assert_eq!(
            evaluate_status_transition(Some(true), true, "R", 5, ALL_ON).1,
            None
        );
    }

    // Swift source: AlertEngineTests.statusFiresUpAndDownOnTransition
    #[test]
    fn status_fires_up_and_down_on_transition() {
        assert_eq!(
            evaluate_status_transition(Some(true), false, "R", 0, ALL_ON),
            (false, Some(AlertDecision::StatusDown { realm: "R".into() }))
        );
        assert_eq!(
            evaluate_status_transition(Some(false), true, "R", 9, ALL_ON),
            (
                true,
                Some(AlertDecision::StatusUp {
                    realm: "R".into(),
                    count: 9
                })
            )
        );
    }

    // Swift source: AlertEngineTests.statusToggleOffSuppressesButStillAdvances
    #[test]
    fn status_toggle_off_suppresses_but_still_advances() {
        let off = AlertSettings {
            status_enabled: false,
            ..ALL_ON
        };
        assert_eq!(
            evaluate_status_transition(Some(true), false, "R", 0, off),
            (false, None)
        );
    }

    // Swift source: AlertEngineTests.debouncedStatusRequiresTwoRemoteFailures
    #[test]
    fn debounced_status_requires_two_remote_failures() {
        let seeded = StatusAlertState {
            confirmed_up: Some(true),
            ..Default::default()
        };
        let first = evaluate_debounced_status(
            seeded,
            StatusAlertObservation::Failure {
                counts_toward_outage: true,
            },
            2,
            "R",
            0,
            ALL_ON,
        );
        assert_eq!(first.0.confirmed_up, Some(true));
        assert_eq!(first.0.consecutive_remote_failures, 1);
        assert_eq!(first.1, None);
        let second = evaluate_debounced_status(
            first.0,
            StatusAlertObservation::Failure {
                counts_toward_outage: true,
            },
            2,
            "R",
            0,
            ALL_ON,
        );
        assert_eq!(second.0.confirmed_up, Some(false));
        assert_eq!(
            second.1,
            Some(AlertDecision::StatusDown { realm: "R".into() })
        );
    }

    // Swift source: AlertEngineTests.localFailureDoesNotAdvanceOrTransitionConfirmedStatus
    // Swift source: StoreTests.repeatedLocalNetworkErrorsNeverClaimRealmDownOrNotify
    #[test]
    fn local_failure_resets_counter_without_moving_confirmed_status() {
        let state = StatusAlertState {
            confirmed_up: Some(true),
            consecutive_remote_failures: 1,
            recovery_notification_armed: false,
        };
        let result = evaluate_debounced_status(
            state,
            StatusAlertObservation::Failure {
                counts_toward_outage: false,
            },
            2,
            "R",
            0,
            ALL_ON,
        );
        assert_eq!(
            result,
            (
                StatusAlertState {
                    confirmed_up: Some(true),
                    ..Default::default()
                },
                None
            )
        );
    }

    // Swift source: AlertEngineTests.healthyZeroPlayerResponseIsARecovery
    #[test]
    fn healthy_zero_player_response_is_a_recovery() {
        let state = StatusAlertState {
            confirmed_up: Some(false),
            consecutive_remote_failures: 2,
            recovery_notification_armed: true,
        };
        let result =
            evaluate_debounced_status(state, StatusAlertObservation::Healthy, 2, "R", 0, ALL_ON);
        assert_eq!(
            result.1,
            Some(AlertDecision::StatusUp {
                realm: "R".into(),
                count: 0
            })
        );
    }

    // Swift source: AlertEngineTests.recoveryIsSilentWhenDownTransitionWasSuppressed
    #[test]
    fn recovery_is_silent_when_down_transition_was_suppressed() {
        let state = StatusAlertState {
            confirmed_up: Some(false),
            consecutive_remote_failures: 2,
            recovery_notification_armed: false,
        };
        assert_eq!(
            evaluate_debounced_status(state, StatusAlertObservation::Healthy, 2, "R", 3, ALL_ON).1,
            None
        );
    }

    // Swift source: AlertEngineTests.peakNoAlertOnFirstEverSample
    #[test]
    fn peak_no_alert_on_first_ever_sample() {
        assert_eq!(
            evaluate_peak(5, date(), 0, None, "R", ALL_ON),
            (5, Some(date()), None)
        );
    }

    // Swift source: AlertEngineTests.peakFiresOnNewHighWithPrior
    #[test]
    fn peak_fires_on_new_high_with_prior() {
        assert_eq!(
            evaluate_peak(10, date(), 5, None, "R", ALL_ON).2,
            Some(AlertDecision::Peak {
                realm: "R".into(),
                count: 10
            })
        );
    }

    // Swift source: AlertEngineTests.peakUnchangedWhenNotAHigh
    #[test]
    fn peak_unchanged_when_not_a_high() {
        let prior = date() - Duration::seconds(100);
        assert_eq!(
            evaluate_peak(3, date(), 5, Some(prior), "R", ALL_ON),
            (5, Some(prior), None)
        );
    }

    // Swift source: AlertEngineTests.peakAdvancesButSilentWhenToggleOff
    #[test]
    fn peak_advances_but_silent_when_toggle_off() {
        let off = AlertSettings {
            peak_enabled: false,
            ..ALL_ON
        };
        assert_eq!(
            evaluate_peak(10, date(), 5, None, "R", off),
            (10, Some(date()), None)
        );
    }

    // Swift source: StoreTests.startupLoadMergesASampleRecordedWhileDiskReadIsInFlight
    #[test]
    fn local_record_observations_are_deferred_until_history_load() {
        let mut state = LocalRecordAlertState::default();
        assert_eq!(
            state.observe(PeakObservation {
                count: 7,
                at: date(),
                realm: "R".into()
            }),
            None
        );
        assert!(!state.history_loaded());
        let pending = state.finish_history_load();
        assert_eq!(pending.len(), 1);
        assert_eq!(
            evaluate_peak(
                pending[0].count,
                pending[0].at,
                50,
                Some(date() - Duration::seconds(60)),
                &pending[0].realm,
                ALL_ON
            )
            .2,
            None
        );
    }

    // Swift source: AlertEngineTests.cryptoIgnoresNonPositivePrice
    #[test]
    fn crypto_ignores_nonpositive_price() {
        assert_eq!(evaluate_crypto(0.0, 1.0, 10.0, ALL_ON), (1.0, None));
    }

    // Swift source: AlertEngineTests.cryptoSeedsBaselineWithoutAlerting
    #[test]
    fn crypto_seeds_baseline_without_alerting() {
        assert_eq!(evaluate_crypto(1.5, 0.0, 10.0, ALL_ON), (1.5, None));
    }

    // Swift source: AlertEngineTests.cryptoPumpAndDumpAndBaselineUpdate
    #[test]
    fn crypto_pump_and_dump_and_baseline_update() {
        assert_eq!(
            evaluate_crypto(2.0, 1.0, 10.0, ALL_ON),
            (
                2.0,
                Some(AlertDecision::CryptoPump {
                    percent: 100,
                    price: "2.0".into()
                })
            )
        );
        assert_eq!(
            evaluate_crypto(0.5, 1.0, 10.0, ALL_ON),
            (
                0.5,
                Some(AlertDecision::CryptoDump {
                    percent: 50,
                    price: "0.5".into()
                })
            )
        );
    }

    // Swift source: AlertEngineTests.cryptoBaselineFrozenWhenToggleOff
    #[test]
    fn crypto_baseline_frozen_when_toggle_off() {
        assert_eq!(
            evaluate_crypto(
                2.0,
                1.0,
                10.0,
                AlertSettings {
                    crypto_enabled: false,
                    ..ALL_ON
                }
            ),
            (1.0, None)
        );
    }

    // Swift source: AlertEngineTests.cryptoNoFireWithinThreshold
    #[test]
    fn crypto_no_fire_within_threshold() {
        assert_eq!(evaluate_crypto(1.05, 1.0, 10.0, ALL_ON), (1.0, None));
    }

    // Swift source: AlertEngineTests.cryptoFiresAtExactThresholdBoundaryNotJustBeyond
    #[test]
    fn crypto_fires_at_exact_threshold_boundary_not_just_beyond() {
        assert!(matches!(
            evaluate_crypto(1.10, 1.0, 10.0, ALL_ON).1,
            Some(AlertDecision::CryptoPump { percent: 10, .. })
        ));
        assert_eq!(evaluate_crypto(1.099, 1.0, 10.0, ALL_ON).1, None);
        assert!(matches!(
            evaluate_crypto(0.90, 1.0, 10.0, ALL_ON).1,
            Some(AlertDecision::CryptoDump { percent: 10, .. })
        ));
        assert_eq!(evaluate_crypto(0.901, 1.0, 10.0, ALL_ON).1, None);
    }

    // Swift source: AlertEngineTests.requestIDAppendsTimestamp
    #[test]
    fn request_id_appends_injected_timestamp() {
        assert_eq!(AlertKind::Status.request_id(|| 1234.5), "status-1234.5");
        assert_eq!(AlertKind::Status.request_id(|| 1234.0), "status-1234.0");
        assert_eq!(
            AlertKind::CryptoPump.request_id(|| 1234.5),
            "crypto_pump-1234.5"
        );
    }
}
