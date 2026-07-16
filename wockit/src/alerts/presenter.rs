//! Pure notification presentation matching the macOS String Catalog fallbacks.

use crate::alerts::{
    core::AlertDecision,
    policy::{ChangeDirection, ChangeWindow, Decision, Payload, ThresholdDirection},
};
use crate::formatting::{chart_price, crypto_price};
use crate::strings;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertContent {
    pub title: String,
    pub body: String,
}

pub fn content(decision: &AlertDecision) -> AlertContent {
    let (title, body) = match decision {
        AlertDecision::StatusUp { realm, count } => strings::alert_status_up(realm, *count),
        AlertDecision::StatusDown { realm } => strings::alert_status_down(realm),
        AlertDecision::Peak { realm, count } => strings::alert_peak(realm, *count),
        AlertDecision::CryptoPump { percent, price } => {
            strings::alert_crypto(true, *percent, price)
        }
        AlertDecision::CryptoDump { percent, price } => {
            strings::alert_crypto(false, *percent, price)
        }
    };
    AlertContent { title, body }
}

pub fn advanced_content(decision: &Decision) -> AlertContent {
    match &decision.payload {
        Payload::Population {
            direction,
            count,
            threshold,
        } => match direction {
            ThresholdDirection::Above => pair(strings::alert_population(true, *count, *threshold)),
            ThresholdDirection::Below => pair(strings::alert_population(false, *count, *threshold)),
        },
        Payload::Price {
            direction,
            price,
            target,
        } => {
            let price = crypto_price(&chart_price(*price));
            let target = crypto_price(&chart_price(*target));
            pair(strings::alert_price(
                *direction == ThresholdDirection::Above,
                &price,
                &target,
            ))
        }
        Payload::Change {
            direction,
            window,
            change_percent,
            threshold_percent,
            price,
        } => {
            let (adjective, duration) = window_copy(*window);
            let price = crypto_price(&chart_price(*price));
            match direction {
                ChangeDirection::Gain => pair(strings::alert_change(
                    true,
                    adjective,
                    duration,
                    *change_percent,
                    &price,
                    *threshold_percent,
                )),
                ChangeDirection::Loss => pair(strings::alert_change(
                    false,
                    adjective,
                    duration,
                    *change_percent,
                    &price,
                    *threshold_percent,
                )),
            }
        }
        Payload::Release(release) => {
            let version = release
                .tag
                .as_deref()
                .or(release.name.as_deref())
                .unwrap_or("new release");
            pair(strings::alert_release(
                version.into(),
                release.summary.as_deref().or(release.name.as_deref()),
            ))
        }
    }
}

fn window_copy(window: ChangeWindow) -> (&'static str, &'static str) {
    strings::alert_window(match window {
        ChangeWindow::OneHour => "1h",
        ChangeWindow::SixHours => "6h",
        ChangeWindow::TwentyFourHours => "24h",
    })
}

fn pair((title, body): (String, String)) -> AlertContent {
    AlertContent { title, body }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alerts::policy::{rule_catalog, AdvancedAlertRuleId};
    use chrono::{TimeZone, Utc};

    // Swift source: AlertPresenterTests.statusUpPluralizes
    #[test]
    fn status_up_pluralizes() {
        let five = content(&AlertDecision::StatusUp {
            realm: "Claudemoon".into(),
            count: 5,
        });
        assert_eq!(five.title, "✅ Claudemoon is back");
        assert_eq!(five.body, "5 players online now.");
        assert_eq!(
            content(&AlertDecision::StatusUp {
                realm: "X".into(),
                count: 1
            })
            .body,
            "1 player online now."
        );
    }

    // Swift source: AlertPresenterTests.statusDown, peak, cryptoPump, cryptoDump
    #[test]
    fn legacy_alert_copy_is_byte_identical() {
        assert_eq!(
            content(&AlertDecision::StatusDown {
                realm: "Claudemoon".into()
            }),
            AlertContent {
                title: "⚠️ Claudemoon looks down".into(),
                body: "0 players online or the realm is unreachable.".into()
            }
        );
        assert_eq!(
            content(&AlertDecision::Peak {
                realm: "Claudemoon".into(),
                count: 142
            })
            .body,
            "142 players online — a new record."
        );
        assert_eq!(
            content(&AlertDecision::CryptoPump {
                percent: 47,
                price: "0.0005594".into()
            })
            .body,
            "The price just surged 47% to $0.0005594!"
        );
        assert_eq!(
            content(&AlertDecision::CryptoDump {
                percent: 12,
                price: "0.0004".into()
            })
            .body,
            "The price dropped 12% to $0.0004."
        );
    }

    // Swift source: AdvancedAlertIntegrationTests.advancedPresenterProvidesLocalizedDomainCopy
    #[test]
    fn advanced_presenter_copy_matches_swift() {
        let fired_at = Utc.timestamp_opt(123, 0).unwrap();
        let loss = Decision {
            rule_id: AdvancedAlertRuleId::new(rule_catalog::TOKEN_CHANGE_LOSS),
            fired_at,
            payload: Payload::Change {
                direction: ChangeDirection::Loss,
                window: ChangeWindow::SixHours,
                change_percent: -12.34,
                threshold_percent: 10.0,
                price: 0.0004,
            },
        };
        assert_eq!(advanced_content(&loss).title, "📉 $WOC 6-hour move");
        assert_eq!(
            advanced_content(&loss).body,
            "Down 12.3% over 6 hours at $0.0004000 — beyond your 10.0% alert."
        );
    }
}
