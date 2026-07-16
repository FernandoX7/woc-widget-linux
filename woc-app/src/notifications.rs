//! Alert notification dispatch and action routing.

use notify_rust::{Notification, NotificationResponse};
use std::{
    io,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use wockit::alerts::{
    core::AlertDecision,
    policy::{rule_catalog, AdvancedAlertRuleId, Decision},
    presenter,
};

use crate::{
    app_state::AppState,
    settings::{DefaultsKey, SettingsStore},
    strings,
};

pub const ACTION_DEFAULT: &str = "default";
pub const ACTION_MUTE_ONE_HOUR: &str = "mute-1h";
pub const ACTION_DISABLE_RULE: &str = "disable-rule";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationRequest {
    pub id: u32,
    pub title: String,
    pub body: String,
    pub rule_id: Option<AdvancedAlertRuleId>,
    pub actions: Vec<(&'static str, &'static str)>,
}

pub trait NotificationBackend: Send + Sync + 'static {
    fn show(
        &self,
        request: NotificationRequest,
        actions: Arc<dyn NotificationActions>,
    ) -> Result<(), String>;
}

pub trait NotificationActions: Send + Sync + 'static {
    fn open_dashboard(&self);
    fn mute_one_hour(&self, rule_id: &AdvancedAlertRuleId) -> io::Result<()>;
    fn disable_rule(&self, rule_id: &AdvancedAlertRuleId) -> io::Result<()>;
}

/// notify-rust implementation. Action waits are lightweight async tasks, not one parked OS
/// thread per notification, so daemons that omit close signals do not consume threads.
#[derive(Default)]
pub struct NotifyRustBackend;

impl NotificationBackend for NotifyRustBackend {
    fn show(
        &self,
        request: NotificationRequest,
        actions: Arc<dyn NotificationActions>,
    ) -> Result<(), String> {
        let mut notification = Notification::new();
        notification
            .id(request.id)
            .summary(&request.title)
            .body(&request.body);
        for (id, label) in &request.actions {
            notification.action(id, label);
        }
        let handle = notification.show().map_err(|error| error.to_string())?;
        let rule = request.rule_id;
        tauri::async_runtime::spawn(async move {
            handle
                .wait_for_action_async(|action| {
                    let action = match action {
                        NotificationResponse::Default => ACTION_DEFAULT,
                        NotificationResponse::Action(value) => value.as_str(),
                        NotificationResponse::Reply(_) | NotificationResponse::Closed(_) => return,
                    };
                    route_action(action, rule.as_ref(), actions.as_ref());
                })
                .await;
        });
        Ok(())
    }
}

pub struct NotificationDispatcher<B> {
    backend: B,
    actions: Arc<dyn NotificationActions>,
    next_id: AtomicU32,
}

impl<B: NotificationBackend> NotificationDispatcher<B> {
    pub fn new(backend: B, actions: Arc<dyn NotificationActions>) -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |value| {
                ((value.as_secs() as u32).rotate_left(13) ^ value.subsec_nanos()).max(1)
            });
        Self {
            backend,
            actions,
            next_id: AtomicU32::new(seed),
        }
    }

    pub fn dispatch(&self, decision: &Decision) -> Result<u32, String> {
        let content = presenter::advanced_content(decision);
        self.send(
            content.title,
            content.body,
            Some(decision.rule_id.clone()),
            true,
        )
    }

    pub fn dispatch_legacy(&self, decision: &AlertDecision) -> Result<u32, String> {
        let content = presenter::content(decision);
        let rule = match decision {
            AlertDecision::StatusUp { .. } | AlertDecision::StatusDown { .. } => {
                rule_catalog::REALM_STATUS
            }
            AlertDecision::Peak { .. } => rule_catalog::LOCAL_RECORD,
            AlertDecision::CryptoPump { .. } => rule_catalog::TOKEN_CHANGE_GAIN,
            AlertDecision::CryptoDump { .. } => rule_catalog::TOKEN_CHANGE_LOSS,
        };
        self.send(
            content.title,
            content.body,
            Some(AdvancedAlertRuleId::new(rule)),
            true,
        )
    }

    /// Phase 13 hook. Linux has no notification permission flow to run first.
    pub fn send_test_notification(&self) -> Result<u32, String> {
        self.send(
            strings::TEST_NOTIFICATION_TITLE.into(),
            strings::TEST_NOTIFICATION_BODY.into(),
            None,
            false,
        )
    }

    fn send(
        &self,
        title: String,
        body: String,
        rule_id: Option<AdvancedAlertRuleId>,
        rule_actions: bool,
    ) -> Result<u32, String> {
        let id = self
            .next_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(if current == u32::MAX { 1 } else { current + 1 })
            })
            .unwrap_or(1)
            .max(1);
        let mut actions = vec![(ACTION_DEFAULT, strings::OPEN_DASHBOARD)];
        if rule_actions {
            actions.push((ACTION_MUTE_ONE_HOUR, strings::MUTE_ONE_HOUR));
            actions.push((ACTION_DISABLE_RULE, strings::DISABLE_ALERT));
        }
        self.backend.show(
            NotificationRequest {
                id,
                title,
                body,
                rule_id,
                actions,
            },
            self.actions.clone(),
        )?;
        Ok(id)
    }
}

pub fn route_action(
    action: &str,
    rule_id: Option<&AdvancedAlertRuleId>,
    sink: &dyn NotificationActions,
) {
    match (action, rule_id) {
        (ACTION_DEFAULT, _) => sink.open_dashboard(),
        (ACTION_MUTE_ONE_HOUR, Some(rule)) => {
            let _ = sink.mute_one_hour(rule);
        }
        (ACTION_DISABLE_RULE, Some(rule)) => {
            let _ = sink.disable_rule(rule);
        }
        _ => {}
    }
}

pub struct SettingsNotificationActions<F> {
    settings: Arc<Mutex<SettingsStore>>,
    open_dashboard: F,
}

impl<F> SettingsNotificationActions<F> {
    pub fn new(settings: Arc<Mutex<SettingsStore>>, open_dashboard: F) -> Self {
        Self {
            settings,
            open_dashboard,
        }
    }
}

impl<F: Fn() + Send + Sync + 'static> NotificationActions for SettingsNotificationActions<F> {
    fn open_dashboard(&self) {
        (self.open_dashboard)();
    }

    fn mute_one_hour(&self, rule_id: &AdvancedAlertRuleId) -> io::Result<()> {
        let expiry = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_secs_f64()
            + 3_600.0;
        self.settings
            .lock()
            .unwrap()
            .update(DefaultsKey::AdvancedAlertMutes, |settings| {
                settings
                    .advanced_alert_mutes
                    .insert(rule_id.as_str().to_owned(), expiry);
            })
            .map(|_| ())
    }

    fn disable_rule(&self, rule_id: &AdvancedAlertRuleId) -> io::Result<()> {
        let Some(key) = disable_key(rule_id.as_str()) else {
            return Ok(());
        };
        self.settings
            .lock()
            .unwrap()
            .update(key, |settings| match rule_id.as_str() {
                rule_catalog::REALM_STATUS => settings.alerts_enabled = false,
                rule_catalog::LOCAL_RECORD => settings.peak_alerts_enabled = false,
                rule_catalog::POPULATION => settings.population_threshold_alerts_enabled = false,
                rule_catalog::TOKEN_PRICE_ABOVE => {
                    settings.token_price_above_alerts_enabled = false
                }
                rule_catalog::TOKEN_PRICE_BELOW => {
                    settings.token_price_below_alerts_enabled = false
                }
                rule_catalog::TOKEN_CHANGE_GAIN => {
                    settings.token_change_gain_alerts_enabled = false
                }
                rule_catalog::TOKEN_CHANGE_LOSS => {
                    settings.token_change_loss_alerts_enabled = false
                }
                rule_catalog::RELEASE => settings.release_alerts_enabled = false,
                _ => {}
            })
            .map(|_| ())
    }
}

fn disable_key(rule: &str) -> Option<DefaultsKey> {
    Some(match rule {
        rule_catalog::REALM_STATUS => DefaultsKey::AlertsEnabled,
        rule_catalog::LOCAL_RECORD => DefaultsKey::PeakAlertsEnabled,
        rule_catalog::POPULATION => DefaultsKey::PopulationThresholdAlertsEnabled,
        rule_catalog::TOKEN_PRICE_ABOVE => DefaultsKey::TokenPriceAboveAlertsEnabled,
        rule_catalog::TOKEN_PRICE_BELOW => DefaultsKey::TokenPriceBelowAlertsEnabled,
        rule_catalog::TOKEN_CHANGE_GAIN => DefaultsKey::TokenChangeGainAlertsEnabled,
        rule_catalog::TOKEN_CHANGE_LOSS => DefaultsKey::TokenChangeLossAlertsEnabled,
        rule_catalog::RELEASE => DefaultsKey::ReleaseAlertsEnabled,
        _ => return None,
    })
}

/// Adapter for the managed production service graph, avoiding a second settings owner.
pub struct AppStateNotificationActions<F> {
    state: Arc<AppState>,
    open_dashboard: F,
}

impl<F> AppStateNotificationActions<F> {
    pub fn new(state: Arc<AppState>, open_dashboard: F) -> Self {
        Self {
            state,
            open_dashboard,
        }
    }
}

impl<F: Fn() + Send + Sync + 'static> NotificationActions for AppStateNotificationActions<F> {
    fn open_dashboard(&self) {
        (self.open_dashboard)();
    }

    fn mute_one_hour(&self, rule_id: &AdvancedAlertRuleId) -> io::Result<()> {
        let expiry = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_secs_f64()
            + 3_600.0;
        self.state
            .settings
            .lock()
            .unwrap()
            .update(DefaultsKey::AdvancedAlertMutes, |settings| {
                settings
                    .advanced_alert_mutes
                    .insert(rule_id.as_str().to_owned(), expiry);
            })
            .map(|_| ())
    }

    fn disable_rule(&self, rule_id: &AdvancedAlertRuleId) -> io::Result<()> {
        let Some(key) = disable_key(rule_id.as_str()) else {
            return Ok(());
        };
        self.state
            .settings
            .lock()
            .unwrap()
            .update(key, |settings| match rule_id.as_str() {
                rule_catalog::REALM_STATUS => settings.alerts_enabled = false,
                rule_catalog::LOCAL_RECORD => settings.peak_alerts_enabled = false,
                rule_catalog::POPULATION => settings.population_threshold_alerts_enabled = false,
                rule_catalog::TOKEN_PRICE_ABOVE => {
                    settings.token_price_above_alerts_enabled = false
                }
                rule_catalog::TOKEN_PRICE_BELOW => {
                    settings.token_price_below_alerts_enabled = false
                }
                rule_catalog::TOKEN_CHANGE_GAIN => {
                    settings.token_change_gain_alerts_enabled = false
                }
                rule_catalog::TOKEN_CHANGE_LOSS => {
                    settings.token_change_loss_alerts_enabled = false
                }
                rule_catalog::RELEASE => settings.release_alerts_enabled = false,
                _ => {}
            })
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env, fs,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use wockit::alerts::policy::{Payload, ThresholdDirection};

    #[derive(Default)]
    struct Actions {
        opens: AtomicUsize,
        muted: Mutex<Vec<String>>,
        disabled: Mutex<Vec<String>>,
    }
    impl NotificationActions for Actions {
        fn open_dashboard(&self) {
            self.opens.fetch_add(1, Ordering::Relaxed);
        }
        fn mute_one_hour(&self, id: &AdvancedAlertRuleId) -> io::Result<()> {
            self.muted.lock().unwrap().push(id.as_str().into());
            Ok(())
        }
        fn disable_rule(&self, id: &AdvancedAlertRuleId) -> io::Result<()> {
            self.disabled.lock().unwrap().push(id.as_str().into());
            Ok(())
        }
    }
    #[derive(Default)]
    struct Backend(Mutex<Vec<NotificationRequest>>);
    impl NotificationBackend for Arc<Backend> {
        fn show(
            &self,
            request: NotificationRequest,
            _: Arc<dyn NotificationActions>,
        ) -> Result<(), String> {
            self.0.lock().unwrap().push(request);
            Ok(())
        }
    }
    fn decision() -> Decision {
        Decision {
            rule_id: AdvancedAlertRuleId::new(rule_catalog::POPULATION),
            fired_at: "2027-01-15T08:00:00Z".parse().unwrap(),
            payload: Payload::Population {
                direction: ThresholdDirection::Above,
                count: 50,
                threshold: 40,
            },
        }
    }

    #[test]
    fn every_fire_has_unique_id_default_first_and_rule_actions() {
        let backend = Arc::new(Backend::default());
        let dispatcher = NotificationDispatcher::new(backend.clone(), Arc::new(Actions::default()));
        let first = dispatcher.dispatch(&decision()).unwrap();
        let second = dispatcher.dispatch(&decision()).unwrap();
        assert_ne!(first, second);
        let requests = backend.0.lock().unwrap();
        assert_eq!(
            requests[0].actions,
            vec![
                (ACTION_DEFAULT, "Open Dashboard"),
                (ACTION_MUTE_ONE_HOUR, "Mute for 1 hour"),
                (ACTION_DISABLE_RULE, "Disable this alert")
            ]
        );
        assert_eq!(
            requests[0].rule_id.as_ref().unwrap().as_str(),
            rule_catalog::POPULATION
        );
    }

    #[test]
    fn test_hook_still_has_default_action() {
        let backend = Arc::new(Backend::default());
        NotificationDispatcher::new(backend.clone(), Arc::new(Actions::default()))
            .send_test_notification()
            .unwrap();
        assert_eq!(
            backend.0.lock().unwrap()[0].actions,
            vec![(ACTION_DEFAULT, "Open Dashboard")]
        );
    }

    #[test]
    fn action_router_targets_exactly_one_rule_and_ignores_unknowns() {
        let sink = Actions::default();
        let rule = AdvancedAlertRuleId::new(rule_catalog::TOKEN_CHANGE_GAIN);
        route_action(ACTION_DEFAULT, Some(&rule), &sink);
        route_action(ACTION_MUTE_ONE_HOUR, Some(&rule), &sink);
        route_action(ACTION_DISABLE_RULE, Some(&rule), &sink);
        route_action("unknown", Some(&rule), &sink);
        assert_eq!(sink.opens.load(Ordering::Relaxed), 1);
        assert_eq!(
            *sink.muted.lock().unwrap(),
            vec![rule_catalog::TOKEN_CHANGE_GAIN]
        );
        assert_eq!(
            *sink.disabled.lock().unwrap(),
            vec![rule_catalog::TOKEN_CHANGE_GAIN]
        );
    }

    #[test]
    fn persisted_actions_mute_and_disable_only_selected_rule() {
        let path = env::temp_dir().join(format!(
            "woc-notification-actions-{}.json",
            std::process::id()
        ));
        let store = Arc::new(Mutex::new(SettingsStore::open_at(&path).unwrap()));
        let sink = SettingsNotificationActions::new(store.clone(), || {});
        let rule = AdvancedAlertRuleId::new(rule_catalog::TOKEN_CHANGE_GAIN);
        sink.mute_one_hour(&rule).unwrap();
        sink.disable_rule(&rule).unwrap();
        drop(sink);
        drop(store);
        let reopened = SettingsStore::open_at(&path).unwrap();
        assert!(!reopened.settings().token_change_gain_alerts_enabled);
        assert!(reopened.settings().token_change_loss_alerts_enabled);
        assert!(reopened
            .settings()
            .advanced_alert_mutes
            .contains_key(rule_catalog::TOKEN_CHANGE_GAIN));
        let _ = fs::remove_file(path);
    }
}
