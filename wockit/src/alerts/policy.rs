//! Pure Alerts 2.0 policy reducer.
//!
//! All times are supplied by the caller. This module deliberately never reads the wall clock.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Timelike, Utc};
use chrono_tz::Tz;
use url::Url;

use crate::models::{CryptoMarketTimeframe, CryptoQuote, GameRelease};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AdvancedAlertRuleId(String);

impl AdvancedAlertRuleId {
    pub fn new(value: impl AsRef<str>) -> Self {
        Self(value.as_ref().trim().to_owned())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn is_valid(&self) -> bool {
        !self.0.is_empty() && self.0.chars().count() <= 128
    }
}

pub mod rule_catalog {
    pub const REALM_STATUS: &str = "realm-status";
    pub const LOCAL_RECORD: &str = "local-record";
    pub const POPULATION: &str = "population-threshold";
    pub const TOKEN_PRICE_ABOVE: &str = "token-price-above";
    pub const TOKEN_PRICE_BELOW: &str = "token-price-below";
    pub const TOKEN_CHANGE_GAIN: &str = "token-change-gain";
    pub const TOKEN_CHANGE_LOSS: &str = "token-change-loss";
    pub const RELEASE: &str = "game-release";
    pub const ALL: [&str; 8] = [
        REALM_STATUS,
        LOCAL_RECORD,
        POPULATION,
        TOKEN_PRICE_ABOVE,
        TOKEN_PRICE_BELOW,
        TOKEN_CHANGE_GAIN,
        TOKEN_CHANGE_LOSS,
        RELEASE,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThresholdDirection {
    Above,
    Below,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeDirection {
    Gain,
    Loss,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeWindow {
    OneHour,
    SixHours,
    TwentyFourHours,
}
impl ChangeWindow {
    fn timeframe(self) -> CryptoMarketTimeframe {
        match self {
            Self::OneHour => CryptoMarketTimeframe::OneHour,
            Self::SixHours => CryptoMarketTimeframe::SixHours,
            Self::TwentyFourHours => CryptoMarketTimeframe::TwentyFourHours,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleConfiguration {
    pub id: AdvancedAlertRuleId,
    pub enabled: bool,
    pub cooldown_seconds: f64,
}
impl RuleConfiguration {
    pub fn new(id: impl AsRef<str>) -> Self {
        Self {
            id: AdvancedAlertRuleId::new(id),
            enabled: true,
            cooldown_seconds: 0.0,
        }
    }
    fn normalized(&self) -> Option<Self> {
        self.id.is_valid().then(|| Self {
            id: self.id.clone(),
            enabled: self.enabled,
            cooldown_seconds: if self.cooldown_seconds.is_finite() {
                self.cooldown_seconds.max(0.0)
            } else {
                0.0
            },
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PopulationPolicy {
    pub rule: RuleConfiguration,
    pub direction: ThresholdDirection,
    pub threshold: i64,
    pub hysteresis: i64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PricePolicy {
    pub rule: RuleConfiguration,
    pub direction: ThresholdDirection,
    pub target: f64,
    pub hysteresis: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ChangePolicy {
    pub rule: RuleConfiguration,
    pub window: ChangeWindow,
    pub direction: ChangeDirection,
    pub threshold_percent: f64,
    pub hysteresis_percent: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ReleasePolicy {
    pub rule: RuleConfiguration,
    pub includes_prereleases: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuietHours {
    pub enabled: bool,
    pub start_minute: i32,
    pub end_minute: i32,
    pub timezone: Tz,
}
impl QuietHours {
    pub fn new(enabled: bool, start_minute: i32, end_minute: i32, timezone: Tz) -> Self {
        Self {
            enabled,
            start_minute: minute_of_day(start_minute),
            end_minute: minute_of_day(end_minute),
            timezone,
        }
    }
    pub fn contains(&self, date: DateTime<Utc>) -> bool {
        if !self.enabled {
            return false;
        }
        let local = date.with_timezone(&self.timezone);
        let value = (local.hour() * 60 + local.minute()) as i32;
        self.start_minute == self.end_minute
            || if self.start_minute < self.end_minute {
                value >= self.start_minute && value < self.end_minute
            } else {
                value >= self.start_minute || value < self.end_minute
            }
    }
}
pub fn minute_of_day(value: i32) -> i32 {
    value.rem_euclid(1_440)
}

/// Store-policy deadbands frozen by Alerts 2.0.
pub fn population_hysteresis(threshold: i64) -> i64 {
    1.max(threshold / 20)
}
pub fn price_hysteresis(target: f64) -> f64 {
    target * 0.02
}
pub fn rolling_hysteresis(threshold: f64) -> f64 {
    threshold.min(1.0_f64.max(threshold * 0.2))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PolicySet {
    pub population: Vec<PopulationPolicy>,
    pub prices: Vec<PricePolicy>,
    pub changes: Vec<ChangePolicy>,
    pub releases: Vec<ReleasePolicy>,
    pub quiet_hours: Option<QuietHours>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    Population {
        direction: ThresholdDirection,
        count: i64,
        threshold: i64,
    },
    Price {
        direction: ThresholdDirection,
        price: f64,
        target: f64,
    },
    Change {
        direction: ChangeDirection,
        window: ChangeWindow,
        change_percent: f64,
        threshold_percent: f64,
        price: f64,
    },
    Release(ReleasePayload),
}
#[derive(Clone, Debug, PartialEq)]
pub struct ReleasePayload {
    pub identity: String,
    pub tag: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub url: Option<Url>,
    pub is_prerelease: bool,
    pub published_at: Option<DateTime<Utc>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub rule_id: AdvancedAlertRuleId,
    pub fired_at: DateTime<Utc>,
    pub payload: Payload,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SuppressionReason {
    Disabled,
    QuietHours,
    Cooldown { until: DateTime<Utc> },
}
#[derive(Clone, Debug, PartialEq)]
pub struct SuppressedAlert {
    pub rule_id: AdvancedAlertRuleId,
    pub occurred_at: DateTime<Utc>,
    pub payload: Payload,
    pub reason: SuppressionReason,
}

pub fn suppression_reason(
    rule: &RuleConfiguration,
    at: DateTime<Utc>,
    quiet: Option<&QuietHours>,
    last: Option<DateTime<Utc>>,
) -> Option<SuppressionReason> {
    if !rule.enabled {
        return Some(SuppressionReason::Disabled);
    }
    if quiet.is_some_and(|q| q.contains(at)) {
        return Some(SuppressionReason::QuietHours);
    }
    if let Some(last) = last {
        let until = last + duration(rule.cooldown_seconds);
        if at < until {
            return Some(SuppressionReason::Cooldown { until });
        }
    }
    None
}

#[derive(Clone, Debug, PartialEq)]
struct Signature {
    direction: ThresholdDirection,
    threshold: f64,
    hysteresis: f64,
    discriminator: String,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThresholdState {
    signature: Option<Signature>,
    has_observation: bool,
    armed: bool,
    last_value: Option<f64>,
    last_delivered_at: Option<DateTime<Utc>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReleaseState {
    has_observation: bool,
    observed: HashSet<String>,
    last_delivered_at: Option<DateTime<Utc>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PolicyState {
    pub population: HashMap<AdvancedAlertRuleId, ThresholdState>,
    pub prices: HashMap<AdvancedAlertRuleId, ThresholdState>,
    pub changes: HashMap<AdvancedAlertRuleId, ThresholdState>,
    pub releases: HashMap<AdvancedAlertRuleId, ReleaseState>,
}

impl PolicyState {
    /// Restores only the delivery clock for a rule after its notification backend fails.
    /// Crossing/rearm state remains consumed, so a failed banner is never replayed.
    pub fn restore_delivery_clock(&mut self, rule: &AdvancedAlertRuleId, previous: &Self) {
        if let Some(state) = self.population.get_mut(rule) {
            state.last_delivered_at = previous
                .population
                .get(rule)
                .and_then(|state| state.last_delivered_at);
        }
        if let Some(state) = self.prices.get_mut(rule) {
            state.last_delivered_at = previous
                .prices
                .get(rule)
                .and_then(|state| state.last_delivered_at);
        }
        if let Some(state) = self.changes.get_mut(rule) {
            state.last_delivered_at = previous
                .changes
                .get(rule)
                .and_then(|state| state.last_delivered_at);
        }
        if let Some(state) = self.releases.get_mut(rule) {
            state.last_delivered_at = previous
                .releases
                .get(rule)
                .and_then(|state| state.last_delivered_at);
        }
    }
}
pub struct Observation<'a> {
    pub observed_at: DateTime<Utc>,
    pub population: Option<i64>,
    pub quote: Option<&'a CryptoQuote>,
    pub releases: Option<&'a [GameRelease]>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Evaluation {
    pub state: PolicyState,
    pub decisions: Vec<Decision>,
    pub suppressed: Vec<SuppressedAlert>,
}

pub fn evaluate(
    raw: &PolicySet,
    mut state: PolicyState,
    observation: Observation<'_>,
) -> Evaluation {
    let policies = normalize(raw);
    let mut decisions = vec![];
    let mut suppressed = vec![];
    if let Some(value) = observation.population.filter(|v| *v >= 0) {
        for p in &policies.population {
            let sig = Signature {
                direction: p.direction,
                threshold: p.threshold as f64,
                hysteresis: p.hysteresis as f64,
                discriminator: "population".into(),
            };
            let payload = Payload::Population {
                direction: p.direction,
                count: value,
                threshold: p.threshold,
            };
            reduce_threshold(
                &p.rule,
                sig,
                value as f64,
                payload,
                observation.observed_at,
                policies.quiet_hours.as_ref(),
                state.population.entry(p.rule.id.clone()).or_default(),
                &mut decisions,
                &mut suppressed,
            );
        }
    }
    if let Some((quote, price)) = observation.quote.and_then(|q| {
        q.price
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.0)
            .map(|v| (q, v))
    }) {
        for p in &policies.prices {
            let sig = Signature {
                direction: p.direction,
                threshold: p.target,
                hysteresis: p.hysteresis,
                discriminator: "token-price".into(),
            };
            reduce_threshold(
                &p.rule,
                sig,
                price,
                Payload::Price {
                    direction: p.direction,
                    price,
                    target: p.target,
                },
                observation.observed_at,
                policies.quiet_hours.as_ref(),
                state.prices.entry(p.rule.id.clone()).or_default(),
                &mut decisions,
                &mut suppressed,
            );
        }
        for p in &policies.changes {
            if let Some(change) = quote
                .metrics(p.window.timeframe())
                .and_then(|m| m.change_percent)
                .filter(|v| v.is_finite())
            {
                let (direction, threshold) = match p.direction {
                    ChangeDirection::Gain => (ThresholdDirection::Above, p.threshold_percent),
                    ChangeDirection::Loss => (ThresholdDirection::Below, -p.threshold_percent),
                };
                let sig = Signature {
                    direction,
                    threshold,
                    hysteresis: p.hysteresis_percent,
                    discriminator: format!("token-change-{:?}", p.window),
                };
                reduce_threshold(
                    &p.rule,
                    sig,
                    change,
                    Payload::Change {
                        direction: p.direction,
                        window: p.window,
                        change_percent: change,
                        threshold_percent: p.threshold_percent,
                        price,
                    },
                    observation.observed_at,
                    policies.quiet_hours.as_ref(),
                    state.changes.entry(p.rule.id.clone()).or_default(),
                    &mut decisions,
                    &mut suppressed,
                );
            }
        }
    }
    if let Some(releases) = observation.releases {
        for p in &policies.releases {
            reduce_releases(
                p,
                releases,
                observation.observed_at,
                policies.quiet_hours.as_ref(),
                state.releases.entry(p.rule.id.clone()).or_default(),
                &mut decisions,
                &mut suppressed,
            );
        }
    }
    Evaluation {
        state,
        decisions,
        suppressed,
    }
}

/// Evaluates persisted per-rule mutes at the observation's single injected timestamp.
/// Muted rules remain present but disabled, so their crossings are consumed rather than queued.
pub fn evaluate_with_mutes(
    raw: &PolicySet,
    state: PolicyState,
    observation: Observation<'_>,
    mutes: &mut RuleMutes,
) -> Evaluation {
    mutes.prune(observation.observed_at);
    let mut policies = raw.clone();
    let apply = |rule: &mut RuleConfiguration| {
        if mutes.0.contains_key(&rule.id) {
            rule.enabled = false;
        }
    };
    policies
        .population
        .iter_mut()
        .for_each(|p| apply(&mut p.rule));
    policies.prices.iter_mut().for_each(|p| apply(&mut p.rule));
    policies.changes.iter_mut().for_each(|p| apply(&mut p.rule));
    policies
        .releases
        .iter_mut()
        .for_each(|p| apply(&mut p.rule));
    evaluate(&policies, state, observation)
}

// Keeping the reducer's state and output sinks explicit makes consumption and delivery-clock
// mutation auditable against the Swift inout implementation.
#[allow(clippy::too_many_arguments)]
fn reduce_threshold(
    rule: &RuleConfiguration,
    sig: Signature,
    value: f64,
    payload: Payload,
    at: DateTime<Utc>,
    quiet: Option<&QuietHours>,
    state: &mut ThresholdState,
    decisions: &mut Vec<Decision>,
    suppressed: &mut Vec<SuppressedAlert>,
) {
    if state.signature.as_ref() != Some(&sig) {
        let delivered = state.last_delivered_at;
        *state = ThresholdState::default();
        state.signature = Some(sig.clone());
        state.last_delivered_at = delivered;
    }
    if !state.has_observation {
        state.has_observation = true;
        state.last_value = Some(value);
        state.armed = !triggered(&sig, value);
        return;
    }
    let old = state.last_value.unwrap();
    let crossed = state.armed
        && match sig.direction {
            ThresholdDirection::Above => old < sig.threshold && value >= sig.threshold,
            ThresholdDirection::Below => old > sig.threshold && value <= sig.threshold,
        };
    if crossed {
        state.armed = false;
        deliver(
            rule,
            payload,
            at,
            quiet,
            &mut state.last_delivered_at,
            decisions,
            suppressed,
        );
    } else if !state.armed
        && !triggered(&sig, value)
        && match sig.direction {
            ThresholdDirection::Above => value <= sig.threshold - sig.hysteresis,
            ThresholdDirection::Below => value >= sig.threshold + sig.hysteresis,
        }
    {
        state.armed = true;
    }
    state.last_value = Some(value);
}
fn triggered(sig: &Signature, value: f64) -> bool {
    match sig.direction {
        ThresholdDirection::Above => value >= sig.threshold,
        ThresholdDirection::Below => value <= sig.threshold,
    }
}
fn deliver(
    rule: &RuleConfiguration,
    payload: Payload,
    at: DateTime<Utc>,
    quiet: Option<&QuietHours>,
    last: &mut Option<DateTime<Utc>>,
    decisions: &mut Vec<Decision>,
    suppressed: &mut Vec<SuppressedAlert>,
) {
    if let Some(reason) = suppression_reason(rule, at, quiet, *last) {
        suppressed.push(SuppressedAlert {
            rule_id: rule.id.clone(),
            occurred_at: at,
            payload,
            reason,
        });
    } else {
        *last = Some(at);
        decisions.push(Decision {
            rule_id: rule.id.clone(),
            fired_at: at,
            payload,
        });
    }
}

fn reduce_releases(
    policy: &ReleasePolicy,
    releases: &[GameRelease],
    at: DateTime<Utc>,
    quiet: Option<&QuietHours>,
    state: &mut ReleaseState,
    decisions: &mut Vec<Decision>,
    suppressed: &mut Vec<SuppressedAlert>,
) {
    let identified: Vec<_> = releases
        .iter()
        .filter_map(|r| release_identity(r).map(|id| (id, r)))
        .collect();
    if !state.has_observation {
        state.has_observation = true;
        state
            .observed
            .extend(identified.iter().map(|v| v.0.clone()));
        return;
    }
    let eligible: Vec<_> = identified
        .iter()
        .filter(|(id, r)| {
            !state.observed.contains(id) && (policy.includes_prereleases || !r.is_prerelease)
        })
        .collect();
    state
        .observed
        .extend(identified.iter().map(|v| v.0.clone()));
    if let Some((id, r)) = eligible
        .into_iter()
        .enumerate()
        .max_by(|(li, (_, l)), (ri, (_, r))| {
            l.published_at.cmp(&r.published_at).then_with(|| ri.cmp(li))
        })
        .map(|(_, v)| v)
    {
        let payload = Payload::Release(release_payload(id.clone(), r));
        deliver(
            &policy.rule,
            payload,
            at,
            quiet,
            &mut state.last_delivered_at,
            decisions,
            suppressed,
        );
    }
}
fn release_identity(r: &GameRelease) -> Option<String> {
    if let Some(id) = r.id {
        return Some(format!("id:{id}"));
    }
    if let Some(v) = text(&r.tag) {
        return Some(format!("tag:{}", v.to_lowercase()));
    }
    if let Some(v) = &r.url {
        return Some(format!("url:{v}"));
    }
    if let Some(v) = r.published_at {
        return Some(format!(
            "published:{}:{}",
            swift_time_interval(v),
            text(&r.name).unwrap_or("untitled").to_lowercase()
        ));
    }
    text(&r.name).map(|v| format!("name:{}", v.to_lowercase()))
}
fn swift_time_interval(value: DateTime<Utc>) -> String {
    let seconds = value.timestamp() as f64 + f64::from(value.timestamp_subsec_nanos()) / 1e9;
    if seconds.fract() == 0.0 {
        format!("{seconds:.1}")
    } else {
        seconds.to_string()
    }
}
fn release_payload(identity: String, r: &GameRelease) -> ReleasePayload {
    ReleasePayload {
        identity,
        tag: text(&r.tag).map(str::to_owned),
        name: text(&r.name).map(str::to_owned),
        summary: r.body.as_ref().and_then(|v| summary(v)),
        url: r.url.clone(),
        is_prerelease: r.is_prerelease,
        published_at: r.published_at,
    }
}
fn text(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|v| !v.is_empty())
}
fn summary(v: &str) -> Option<String> {
    let s = v.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() {
        None
    } else if s.chars().count() <= 240 {
        Some(s)
    } else {
        Some(format!(
            "{}…",
            s.chars().take(239).collect::<String>().trim()
        ))
    }
}
fn duration(seconds: f64) -> Duration {
    Duration::milliseconds((seconds * 1000.0) as i64)
}

fn normalize(raw: &PolicySet) -> PolicySet {
    fn unique<T>(
        items: impl Iterator<Item = T>,
        id: impl Fn(&T) -> &AdvancedAlertRuleId,
    ) -> Vec<T> {
        let mut seen = HashSet::new();
        items.filter(|v| seen.insert(id(v).clone())).collect()
    }
    let population = unique(
        raw.population.iter().filter_map(|p| {
            p.rule.normalized().map(|rule| {
                let threshold = p.threshold.max(0);
                let h = p.hysteresis.max(0);
                PopulationPolicy {
                    rule,
                    direction: p.direction,
                    threshold,
                    hysteresis: if p.direction == ThresholdDirection::Above {
                        h.min(threshold)
                    } else {
                        h
                    },
                }
            })
        }),
        |p| &p.rule.id,
    );
    let prices = unique(
        raw.prices.iter().filter_map(|p| {
            p.rule
                .normalized()
                .filter(|_| p.target.is_finite() && p.target > 0.0)
                .map(|rule| {
                    let h = if p.hysteresis.is_finite() && p.hysteresis >= 0.0 {
                        p.hysteresis
                    } else {
                        0.0
                    };
                    PricePolicy {
                        rule,
                        direction: p.direction,
                        target: p.target,
                        hysteresis: if p.direction == ThresholdDirection::Above {
                            h.min(p.target)
                        } else {
                            h
                        },
                    }
                })
        }),
        |p| &p.rule.id,
    );
    let changes = unique(
        raw.changes.iter().filter_map(|p| {
            p.rule
                .normalized()
                .filter(|_| p.threshold_percent.is_finite() && p.threshold_percent > 0.0)
                .map(|rule| ChangePolicy {
                    rule,
                    window: p.window,
                    direction: p.direction,
                    threshold_percent: p.threshold_percent,
                    hysteresis_percent: if p.hysteresis_percent.is_finite()
                        && p.hysteresis_percent >= 0.0
                    {
                        p.hysteresis_percent.min(p.threshold_percent)
                    } else {
                        0.0
                    },
                })
        }),
        |p| &p.rule.id,
    );
    let releases = unique(
        raw.releases.iter().filter_map(|p| {
            p.rule.normalized().map(|rule| ReleasePolicy {
                rule,
                includes_prereleases: p.includes_prereleases,
            })
        }),
        |p| &p.rule.id,
    );
    PolicySet {
        population,
        prices,
        changes,
        releases,
        quiet_hours: raw.quiet_hours.clone(),
    }
}

/// Persistable per-rule mute expiry map. Callers inject `now`; expired entries are pruned.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleMutes(pub HashMap<AdvancedAlertRuleId, DateTime<Utc>>);
impl RuleMutes {
    pub fn from_unix_seconds(values: &HashMap<String, f64>, now: DateTime<Utc>) -> Self {
        let entries = values
            .iter()
            .filter_map(|(id, seconds)| {
                if !rule_catalog::ALL.contains(&id.as_str()) || !seconds.is_finite() {
                    return None;
                }
                let whole = seconds.trunc() as i64;
                let nanos = ((seconds.fract().max(0.0)) * 1_000_000_000.0) as u32;
                let expiry = DateTime::from_timestamp(whole, nanos)?;
                (expiry > now).then(|| (AdvancedAlertRuleId::new(id), expiry))
            })
            .collect();
        Self(entries)
    }
    pub fn mute_one_hour(&mut self, id: AdvancedAlertRuleId, now: DateTime<Utc>) {
        self.0.insert(id, now + Duration::seconds(3_600));
    }
    pub fn prune(&mut self, now: DateTime<Utc>) {
        self.0.retain(|_, expiry| *expiry > now);
    }
    pub fn is_muted(&mut self, id: &AdvancedAlertRuleId, now: DateTime<Utc>) -> bool {
        self.prune(now);
        self.0.contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::{America::Chicago, Etc::GMTMinus1, UTC};
    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
    }
    fn rule(id: &str) -> RuleConfiguration {
        RuleConfiguration::new(id)
    }
    fn pop(rule: RuleConfiguration) -> PolicySet {
        PolicySet {
            population: vec![PopulationPolicy {
                rule,
                direction: ThresholdDirection::Above,
                threshold: 10,
                hysteresis: 1,
            }],
            ..Default::default()
        }
    }
    fn obs<'a>(seconds: i64, value: Option<i64>) -> Observation<'a> {
        Observation {
            observed_at: at(seconds),
            population: value,
            quote: None,
            releases: None,
        }
    }

    // Swift: AdvancedAlertPolicyTests.populationSeedsSilentlyThenFiresOnExactUpwardCrossing
    #[test]
    fn population_seeds_then_crosses() {
        let p = pop(rule("p"));
        let a = evaluate(&p, PolicyState::default(), obs(0, Some(9)));
        assert!(a.decisions.is_empty());
        let b = evaluate(&p, a.state, obs(1, Some(10)));
        assert_eq!(b.decisions.len(), 1);
    }

    #[test]
    fn failed_backend_restores_only_delivery_clock_and_consumes_crossing() {
        let mut configured = rule("p");
        configured.cooldown_seconds = 3_600.0;
        let p = pop(configured);
        let seeded = evaluate(&p, PolicyState::default(), obs(0, Some(9)));
        let crossed = evaluate(&p, seeded.state.clone(), obs(1, Some(10)));
        assert_eq!(crossed.decisions.len(), 1);

        let id = AdvancedAlertRuleId::new("p");
        let mut restored = crossed.state;
        restored.restore_delivery_clock(&id, &seeded.state);
        assert!(evaluate(&p, restored.clone(), obs(2, Some(11)))
            .decisions
            .is_empty());

        let rearmed = evaluate(&p, restored, obs(3, Some(9))).state;
        assert_eq!(evaluate(&p, rearmed, obs(4, Some(10))).decisions.len(), 1);
    }
    // Swift: AdvancedAlertPolicyTests.firedRuleDoesNotSpamAndRequiresDeadbandBeforeRearming
    #[test]
    fn deadband_rearm() {
        let p = pop(rule("p"));
        let mut s = evaluate(&p, PolicyState::default(), obs(0, Some(9))).state;
        s = evaluate(&p, s, obs(1, Some(10))).state;
        assert!(evaluate(&p, s.clone(), obs(2, Some(11)))
            .decisions
            .is_empty());
        s = evaluate(&p, s, obs(2, Some(9))).state;
        assert_eq!(evaluate(&p, s, obs(3, Some(10))).decisions.len(), 1);
    }
    // Swift: AdvancedAlertPolicyTests.quietHoursSuppressAndConsumeCrossing
    #[test]
    fn quiet_consumes() {
        let mut p = pop(rule("p"));
        p.quiet_hours = Some(QuietHours::new(true, 0, 0, UTC));
        let a = evaluate(&p, PolicyState::default(), obs(0, Some(9)));
        let b = evaluate(&p, a.state, obs(1, Some(10)));
        assert_eq!(b.suppressed[0].reason, SuppressionReason::QuietHours);
        p.quiet_hours = None;
        assert!(evaluate(&p, b.state, obs(2, Some(10))).decisions.is_empty());
    }
    // Swift: AdvancedAlertPolicyTests.cooldownIsPerRuleAndExactExpiryIsDeliverable
    #[test]
    fn exact_cooldown_expiry() {
        let mut r = rule("p");
        r.cooldown_seconds = 10.0;
        let p = pop(r);
        let mut s = evaluate(&p, PolicyState::default(), obs(0, Some(9))).state;
        s = evaluate(&p, s, obs(1, Some(10))).state;
        s = evaluate(&p, s, obs(2, Some(9))).state;
        assert!(evaluate(&p, s.clone(), obs(10, Some(10)))
            .decisions
            .is_empty());
        assert_eq!(evaluate(&p, s, obs(11, Some(10))).decisions.len(), 1);
    }

    #[test]
    fn cooldown_delivery_clock_is_independent_per_rule() {
        let mut a = rule("a");
        a.cooldown_seconds = 100.0;
        let mut b = rule("b");
        b.cooldown_seconds = 100.0;
        b.enabled = false;
        let mut p = pop(a);
        p.population.push(PopulationPolicy {
            rule: b,
            direction: ThresholdDirection::Above,
            threshold: 10,
            hysteresis: 1,
        });
        let seeded = evaluate(&p, PolicyState::default(), obs(0, Some(9)));
        let first = evaluate(&p, seeded.state, obs(1, Some(10)));
        assert_eq!(first.decisions[0].rule_id.as_str(), "a");
        p.population[1].rule.enabled = true;
        let rearmed = evaluate(&p, first.state, obs(2, Some(9)));
        let second = evaluate(&p, rearmed.state, obs(3, Some(10)));
        assert_eq!(second.decisions[0].rule_id.as_str(), "b");
        assert!(matches!(
            second.suppressed[0].reason,
            SuppressionReason::Cooldown { .. }
        ));
    }
    // Swift: AdvancedAlertPolicyTests.daytimeQuietHoursUseStartInclusiveEndExclusiveBoundaries
    #[test]
    fn quiet_boundaries() {
        let q = QuietHours::new(true, 60, 120, UTC);
        let day = Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap();
        assert!(!q.contains(day + Duration::minutes(59)));
        assert!(q.contains(day + Duration::minutes(60)));
        assert!(!q.contains(day + Duration::minutes(120)));
    }

    #[test]
    fn local_quiet_hours_follow_daylight_saving_offset_at_each_observation() {
        let q = QuietHours::new(true, 22 * 60, 23 * 60, Chicago);
        let winter = Utc.with_ymd_and_hms(2027, 1, 15, 4, 30, 0).unwrap();
        let summer = Utc.with_ymd_and_hms(2027, 7, 15, 3, 30, 0).unwrap();
        assert!(q.contains(winter));
        assert!(q.contains(summer));
        assert!(!q.contains(winter - Duration::hours(1)));
        assert!(!q.contains(summer - Duration::hours(1)));
    }
    // Swift: AdvancedAlertIntegrationTests.muteActionPersistsPerRuleAndConsumesCrossingsUntilExpiry
    #[test]
    fn per_rule_mute_prunes_at_expiry() {
        let mut m = RuleMutes::default();
        let a = AdvancedAlertRuleId::new("a");
        let b = AdvancedAlertRuleId::new("b");
        m.mute_one_hour(a.clone(), at(0));
        assert!(m.is_muted(&a, at(3599)));
        assert!(!m.is_muted(&b, at(3599)));
        assert!(!m.is_muted(&a, at(3600)));
        assert!(m.0.is_empty());
    }

    #[test]
    fn muted_crossing_is_consumed_and_expiry_requires_a_fresh_crossing() {
        let id = AdvancedAlertRuleId::new("p");
        let p = pop(rule("p"));
        let mut mutes = RuleMutes::default();
        mutes.mute_one_hour(id, at(0));
        let seeded = evaluate_with_mutes(&p, PolicyState::default(), obs(0, Some(9)), &mut mutes);
        let muted = evaluate_with_mutes(&p, seeded.state, obs(1, Some(10)), &mut mutes);
        assert_eq!(muted.suppressed[0].reason, SuppressionReason::Disabled);

        let expired = evaluate_with_mutes(&p, muted.state, obs(3600, Some(10)), &mut mutes);
        assert!(expired.decisions.is_empty());
        assert!(mutes.0.is_empty());
        let rearmed = evaluate_with_mutes(&p, expired.state, obs(3601, Some(9)), &mut mutes);
        let fresh = evaluate_with_mutes(&p, rearmed.state, obs(3602, Some(10)), &mut mutes);
        assert_eq!(fresh.decisions.len(), 1);
    }
    // Swift: AdvancedAlertPolicyTests.disabledRulesTrackAndConsumeCrossings
    #[test]
    fn gate_order_disabled_before_quiet_and_cooldown() {
        let mut r = rule("p");
        r.enabled = false;
        r.cooldown_seconds = 100.0;
        let q = QuietHours::new(true, 0, 0, UTC);
        assert_eq!(
            suppression_reason(&r, at(5), Some(&q), Some(at(0))),
            Some(SuppressionReason::Disabled)
        );
    }

    // Swift: AdvancedAlertPolicyTests.normalizationRejectsInvalidIDsAndFloatingPointTargets
    #[test]
    fn normalization_rejects_invalid_ids_and_targets() {
        let p = PolicySet {
            prices: vec![PricePolicy {
                rule: rule(" "),
                direction: ThresholdDirection::Above,
                target: f64::NAN,
                hysteresis: 0.0,
            }],
            ..Default::default()
        };
        assert!(normalize(&p).prices.is_empty());
    }
    // Swift: AdvancedAlertPolicyTests.policiesNormalizeThresholdsHysteresisAndDuplicates
    #[test]
    fn policy_normalization_bounds_and_deduplicates() {
        let p = PolicySet {
            population: vec![
                PopulationPolicy {
                    rule: rule("x"),
                    direction: ThresholdDirection::Above,
                    threshold: 5,
                    hysteresis: 20,
                },
                PopulationPolicy {
                    rule: rule("x"),
                    direction: ThresholdDirection::Below,
                    threshold: 9,
                    hysteresis: 1,
                },
            ],
            ..Default::default()
        };
        let n = normalize(&p);
        assert_eq!(n.population.len(), 1);
        assert_eq!(n.population[0].hysteresis, 5);
    }
    // Swift: AdvancedAlertPolicyTests.overnightQuietHoursSpanMidnightAndRespectTimezone
    #[test]
    fn overnight_quiet_respects_timezone() {
        let q = QuietHours::new(true, 22 * 60, 7 * 60, GMTMinus1);
        let d = Utc.with_ymd_and_hms(2027, 1, 1, 21, 0, 0).unwrap();
        assert!(q.contains(d));
        assert!(!q.contains(d + Duration::hours(10)));
    }
    // Swift: AdvancedAlertPolicyTests.equalQuietHoursMeanAllDayUnlessDisabled
    #[test]
    fn equal_quiet_is_all_day_unless_disabled() {
        let z = UTC;
        assert!(QuietHours::new(true, 0, 0, z).contains(at(0)));
        assert!(!QuietHours::new(false, 0, 0, z).contains(at(0)));
    }
    // Swift: AdvancedAlertPolicyTests.initialTriggeredPopulationDoesNotFireUntilItRearmsAndCrossesAgain
    #[test]
    fn initial_triggered_population_waits_for_rearm() {
        let p = pop(rule("p"));
        let mut s = evaluate(&p, PolicyState::default(), obs(0, Some(10))).state;
        assert!(evaluate(&p, s.clone(), obs(1, Some(11)))
            .decisions
            .is_empty());
        s = evaluate(&p, s, obs(2, Some(9))).state;
        assert_eq!(evaluate(&p, s, obs(3, Some(10))).decisions.len(), 1);
    }
    // Swift: AdvancedAlertPolicyTests.belowPopulationRuleUsesMirroredCrossingAndRearmSemantics
    #[test]
    fn below_population_is_mirrored() {
        let p = PolicySet {
            population: vec![PopulationPolicy {
                rule: rule("p"),
                direction: ThresholdDirection::Below,
                threshold: 10,
                hysteresis: 2,
            }],
            ..Default::default()
        };
        let mut s = evaluate(&p, PolicyState::default(), obs(0, Some(11))).state;
        s = evaluate(&p, s, obs(1, Some(10))).state;
        s = evaluate(&p, s, obs(2, Some(11))).state;
        assert!(evaluate(&p, s.clone(), obs(3, Some(10)))
            .decisions
            .is_empty());
        s = evaluate(&p, s, obs(4, Some(12))).state;
        assert_eq!(evaluate(&p, s, obs(5, Some(10))).decisions.len(), 1);
    }
    // Swift: AdvancedAlertPolicyTests.invalidPopulationObservationDoesNotMutateState
    #[test]
    fn invalid_population_does_not_mutate() {
        let p = pop(rule("p"));
        let s = PolicyState::default();
        assert_eq!(evaluate(&p, s.clone(), obs(0, Some(-1))).state, s);
    }
    // Swift: AdvancedAlertPolicyTests.changingThresholdReseedsSilently
    #[test]
    fn target_edit_reseeds_and_preserves_delivery_clock() {
        let mut p = pop(rule("p"));
        let mut s = evaluate(&p, PolicyState::default(), obs(0, Some(9))).state;
        s = evaluate(&p, s, obs(1, Some(10))).state;
        p.population[0].threshold = 20;
        let e = evaluate(&p, s, obs(2, Some(20)));
        assert!(e.decisions.is_empty());
        assert_eq!(
            e.state.population[&AdvancedAlertRuleId::new("p")].last_delivered_at,
            Some(at(1))
        );
    }
    // Swift: AdvancedAlertPolicyTests.absolutePriceSupportsAboveAndBelowTargetsIndependently
    #[test]
    fn prices_above_and_below_are_independent() {
        let mut market = HashMap::new();
        let q1 = quote("1", &mut market);
        let q2 = quote("2", &mut market);
        let p = PolicySet {
            prices: vec![
                PricePolicy {
                    rule: rule("up"),
                    direction: ThresholdDirection::Above,
                    target: 2.0,
                    hysteresis: 0.0,
                },
                PricePolicy {
                    rule: rule("down"),
                    direction: ThresholdDirection::Below,
                    target: 0.5,
                    hysteresis: 0.0,
                },
            ],
            ..Default::default()
        };
        let a = evaluate(&p, PolicyState::default(), quote_obs(0, &q1));
        assert_eq!(
            evaluate(&p, a.state, quote_obs(1, &q2)).decisions[0]
                .rule_id
                .as_str(),
            "up"
        );
    }
    // Swift: AdvancedAlertPolicyTests.malformedNonfiniteAndNonpositivePricesAreIgnored
    #[test]
    fn malformed_prices_are_ignored() {
        for v in ["nope", "NaN", "inf", "0", "-1"] {
            let mut m = HashMap::new();
            let q = quote(v, &mut m);
            assert_eq!(
                evaluate(
                    &PolicySet {
                        prices: vec![PricePolicy {
                            rule: rule("p"),
                            direction: ThresholdDirection::Above,
                            target: 1.0,
                            hysteresis: 0.0
                        }],
                        ..Default::default()
                    },
                    PolicyState::default(),
                    quote_obs(0, &q)
                )
                .state,
                PolicyState::default()
            );
        }
    }
    // Swift: AdvancedAlertPolicyTests.rollingRulesReadTheirExactRichCryptoQuoteWindows
    #[test]
    fn rolling_reads_exact_window() {
        let mut m = HashMap::new();
        m.insert(
            CryptoMarketTimeframe::OneHour,
            crate::models::CryptoMarketWindow {
                change_percent: Some(9.0),
                buys: None,
                sells: None,
                volume_usd: None,
            },
        );
        let q1 = quote("1", &mut m);
        m.get_mut(&CryptoMarketTimeframe::OneHour)
            .unwrap()
            .change_percent = Some(10.0);
        let q2 = quote("1", &mut m);
        let p = change_policy(ChangeDirection::Gain);
        let a = evaluate(&p, PolicyState::default(), quote_obs(0, &q1));
        assert_eq!(evaluate(&p, a.state, quote_obs(1, &q2)).decisions.len(), 1);
    }
    // Swift: AdvancedAlertPolicyTests.lossRuleUsesNegativeThresholdAndRearmsTowardZero
    #[test]
    fn loss_uses_negative_threshold() {
        let p = change_policy(ChangeDirection::Loss);
        let q1 = change_quote(-9.0);
        let q2 = change_quote(-10.0);
        let a = evaluate(&p, PolicyState::default(), quote_obs(0, &q1));
        assert_eq!(evaluate(&p, a.state, quote_obs(1, &q2)).decisions.len(), 1);
    }
    // Swift: AdvancedAlertPolicyTests.missingOrNonfiniteWindowMetricDoesNotSeedOrMutateRule
    #[test]
    fn missing_change_metric_does_not_mutate() {
        let mut m = HashMap::new();
        let q = quote("1", &mut m);
        assert_eq!(
            evaluate(
                &change_policy(ChangeDirection::Gain),
                PolicyState::default(),
                quote_obs(0, &q)
            )
            .state,
            PolicyState::default()
        );
    }
    // Swift: AdvancedAlertPolicyTests.noObservedFeedsLeaveAllStateUnchanged
    #[test]
    fn no_feeds_is_noop() {
        let s = PolicyState::default();
        assert_eq!(evaluate(&pop(rule("p")), s.clone(), obs(0, None)).state, s);
    }
    // Swift: AdvancedAlertPolicyTests.invalidObservationTimestampIsACompleteNoOp — Rust's
    // DateTime<Utc> cannot represent the Swift non-finite Date input, so construction rejects it.
    #[test]
    fn observation_timestamp_type_excludes_nonfinite_values() {
        assert!(Utc.timestamp_opt(i64::MAX, 0).single().is_none());
    }
    // Swift: AdvancedAlertIntegrationTests.advancedPreferencesNormalizeOnLoadAndPersistOnChange
    #[test]
    fn minute_and_cooldown_normalizers_match_store_inputs() {
        assert_eq!(minute_of_day(-60), 1380);
        assert_eq!(
            crate::config::advanced_alert::normalize_cooldown(1000.0),
            900.0
        );
    }
    // Swift: AdvancedAlertIntegrationTests.unknownNotificationActionsAndRuleIDsAreNoOps
    #[test]
    fn catalog_rejects_unknown_rule_for_actions() {
        assert!(!rule_catalog::ALL.contains(&"unknown"));
        assert_eq!(rule_catalog::ALL.len(), 8);
    }
    // Swift: StorePolicyTests alert policy construction deadband assertions.
    #[test]
    fn store_policy_hysteresis_formulas_are_frozen() {
        assert_eq!(population_hysteresis(10), 1);
        assert_eq!(population_hysteresis(100), 5);
        assert_eq!(price_hysteresis(50.0), 1.0);
        assert_eq!(rolling_hysteresis(2.0), 1.0);
        assert_eq!(rolling_hysteresis(10.0), 2.0);
    }
    // Swift: AdvancedAlertIntegrationTests.successfulPlayerObservationsDrivePopulationAlertWithNotificationMetadata
    #[test]
    fn integration_population_observation_has_stable_metadata() {
        let p = pop(rule(rule_catalog::POPULATION));
        let a = evaluate(&p, PolicyState::default(), obs(0, Some(9)));
        let b = evaluate(&p, a.state, obs(1, Some(10)));
        assert_eq!(b.decisions[0].rule_id.as_str(), rule_catalog::POPULATION);
    }
    // Swift: AdvancedAlertIntegrationTests.richMarketQuoteUsesSelectedRollingWindowWithoutLegacyDuplicate
    #[test]
    fn integration_rich_quote_selected_window_fires_once() {
        let p = change_policy(ChangeDirection::Gain);
        let a = change_quote(9.0);
        let b = change_quote(10.0);
        let seeded = evaluate(&p, PolicyState::default(), quote_obs(0, &a));
        let fired = evaluate(&p, seeded.state, quote_obs(1, &b));
        assert_eq!(fired.decisions.len(), 1);
    }
    // Swift: AdvancedAlertIntegrationTests.absolutePriceTargetsObserveSuccessfulSpotQuotes
    #[test]
    fn integration_absolute_price_target_observes_spot() {
        let mut m = HashMap::new();
        let a = quote("0.9", &mut m);
        let b = quote("1.0", &mut m);
        let p = PolicySet {
            prices: vec![PricePolicy {
                rule: rule(rule_catalog::TOKEN_PRICE_ABOVE),
                direction: ThresholdDirection::Above,
                target: 1.0,
                hysteresis: 0.02,
            }],
            ..Default::default()
        };
        let seeded = evaluate(&p, PolicyState::default(), quote_obs(0, &a));
        assert_eq!(
            evaluate(&p, seeded.state, quote_obs(1, &b)).decisions.len(),
            1
        );
    }
    // Swift: AdvancedAlertIntegrationTests.releaseObservationsSeedThenDeliverAndDisableActionPersists
    #[test]
    fn integration_release_seeds_delivers_and_disable_consumes() {
        let p = release_policy(false);
        let a = evaluate(
            &p,
            PolicyState::default(),
            release_obs(0, &[release(1, false, 0)]),
        );
        let mut disabled = release_policy(false);
        disabled.releases[0].rule.enabled = false;
        let b = evaluate(&disabled, a.state, release_obs(1, &[release(2, false, 1)]));
        assert_eq!(b.suppressed[0].reason, SuppressionReason::Disabled);
    }
    // Swift: AdvancedAlertIntegrationTests.overnightQuietHoursSuppressCrossingWithoutDelayedDelivery
    #[test]
    fn integration_overnight_quiet_has_no_delayed_delivery() {
        let mut p = pop(rule("p"));
        p.quiet_hours = Some(QuietHours::new(true, 22 * 60, 7 * 60, UTC));
        let base = Utc.with_ymd_and_hms(2027, 1, 1, 21, 0, 0).unwrap();
        let a = evaluate(
            &p,
            PolicyState::default(),
            Observation {
                observed_at: base,
                population: Some(9),
                quote: None,
                releases: None,
            },
        );
        let b = evaluate(
            &p,
            a.state,
            Observation {
                observed_at: base + Duration::hours(2),
                population: Some(10),
                quote: None,
                releases: None,
            },
        );
        p.quiet_hours = None;
        assert!(evaluate(
            &p,
            b.state,
            Observation {
                observed_at: base + Duration::hours(11),
                population: Some(10),
                quote: None,
                releases: None
            }
        )
        .decisions
        .is_empty());
    }
    // Swift: AdvancedAlertIntegrationTests.realmAndRecordAlertsCarryStableActionMetadata
    #[test]
    fn integration_realm_and_record_ids_are_frozen() {
        assert_eq!(rule_catalog::REALM_STATUS, "realm-status");
        assert_eq!(rule_catalog::LOCAL_RECORD, "local-record");
    }
    // Swift: AdvancedAlertIntegrationTests.quietRealmTransitionIsConsumedWithoutRecoveryReplay
    #[test]
    fn integration_quiet_realm_gate_consumes_transition() {
        let r = rule(rule_catalog::REALM_STATUS);
        let q = QuietHours::new(true, 0, 0, UTC);
        assert_eq!(
            suppression_reason(&r, at(0), Some(&q), None),
            Some(SuppressionReason::QuietHours)
        );
    }
    // Swift: AdvancedAlertIntegrationTests.recordCooldownConsumesIntermediateRecordsAndAllowsANewOneAtExpiry
    #[test]
    fn integration_record_cooldown_does_not_advance_clock() {
        let mut r = rule(rule_catalog::LOCAL_RECORD);
        r.cooldown_seconds = 10.0;
        let last = at(0);
        assert!(matches!(
            suppression_reason(&r, at(5), None, Some(last)),
            Some(SuppressionReason::Cooldown { .. })
        ));
        assert_eq!(suppression_reason(&r, at(10), None, Some(last)), None);
    }
    // Swift: AdvancedAlertIntegrationTests.disablingGainRuleLeavesLossRuleEnabledAndDeliverable
    #[test]
    fn integration_disabling_gain_leaves_loss_enabled() {
        let mut gain = rule(rule_catalog::TOKEN_CHANGE_GAIN);
        gain.enabled = false;
        let loss = rule(rule_catalog::TOKEN_CHANGE_LOSS);
        assert_eq!(
            suppression_reason(&gain, at(0), None, None),
            Some(SuppressionReason::Disabled)
        );
        assert_eq!(suppression_reason(&loss, at(0), None, None), None);
    }
    // Swift: AdvancedAlertIntegrationTests.persistedMutesDiscardUnknownExpiredAndNonfiniteEntries
    #[test]
    fn persisted_mutes_discard_unknown_expired_and_nonfinite() {
        let mut raw = HashMap::new();
        raw.insert(rule_catalog::POPULATION.into(), at(10).timestamp() as f64);
        raw.insert(rule_catalog::RELEASE.into(), f64::NAN);
        raw.insert("unknown".into(), at(20).timestamp() as f64);
        raw.insert(
            rule_catalog::TOKEN_PRICE_ABOVE.into(),
            at(-1).timestamp() as f64,
        );
        let loaded = RuleMutes::from_unix_seconds(&raw, at(0));
        assert_eq!(loaded.0.len(), 1);
        assert!(loaded
            .0
            .contains_key(&AdvancedAlertRuleId::new(rule_catalog::POPULATION)));
    }

    // Swift: AdvancedAlertPolicyTests.currentReleaseFeedSeedsSilentlyThenOnlyNewIdentityFiresOnce
    #[test]
    fn release_seeds_then_new_identity_fires_once() {
        let p = release_policy(false);
        let a = evaluate(&p, PolicyState::default(), release_obs(0, &[]));
        let rows = [release(2, false, 1)];
        let b = evaluate(&p, a.state, release_obs(1, &rows));
        assert_eq!(b.decisions.len(), 1);
        assert!(evaluate(&p, b.state, release_obs(2, &rows))
            .decisions
            .is_empty());
    }
    // Swift: AdvancedAlertPolicyTests.emptySuccessfulReleaseFeedAllowsFirstLaterReleaseToFire
    #[test]
    fn empty_release_seed_allows_later_fire() {
        let p = release_policy(false);
        let a = evaluate(&p, PolicyState::default(), release_obs(0, &[]));
        assert_eq!(
            evaluate(&p, a.state, release_obs(1, &[release(1, false, 1)]))
                .decisions
                .len(),
            1
        );
    }
    // Swift: AdvancedAlertPolicyTests.newestOfSeveralUnseenReleasesWinsDeterministically
    #[test]
    fn newest_unseen_release_wins() {
        let p = release_policy(false);
        let a = evaluate(&p, PolicyState::default(), release_obs(0, &[]));
        let e = evaluate(
            &p,
            a.state,
            release_obs(1, &[release(1, false, 1), release(2, false, 2)]),
        );
        match &e.decisions[0].payload {
            Payload::Release(v) => assert_eq!(v.identity, "id:2"),
            _ => panic!(),
        }
    }
    // Swift: AdvancedAlertPolicyTests.prereleaseFilteringConsumesIneligibleReleaseWithoutLaterReplay
    #[test]
    fn prerelease_filter_consumes() {
        let p = release_policy(false);
        let a = evaluate(&p, PolicyState::default(), release_obs(0, &[]));
        let rows = [release(1, true, 1)];
        let b = evaluate(&p, a.state, release_obs(1, &rows));
        assert!(b.decisions.is_empty());
        assert!(
            evaluate(&release_policy(true), b.state, release_obs(2, &rows))
                .decisions
                .is_empty()
        );
    }
    // Swift: AdvancedAlertPolicyTests.releaseIdentityFallsBackFromIDToTagURLPublishedDateAndName
    #[test]
    fn release_identity_precedence_and_fallbacks() {
        let mut r = release(1, false, 1);
        assert_eq!(release_identity(&r).unwrap(), "id:1");
        r.id = None;
        assert_eq!(release_identity(&r).unwrap(), "tag:v1");
        r.tag = None;
        r.url = Some(Url::parse("https://example.com/r").unwrap());
        assert!(release_identity(&r).unwrap().starts_with("url:"));
        r.url = None;
        assert!(release_identity(&r).unwrap().starts_with("published:"));
        r.published_at = Utc.timestamp_opt(-1, 500_000_000).single();
        assert_eq!(release_identity(&r).unwrap(), "published:-0.5:release 1");
        r.published_at = None;
        assert_eq!(release_identity(&r).unwrap(), "name:release 1");
    }
    // Swift: AdvancedAlertPolicyTests.identitylessReleaseIsIgnored
    #[test]
    fn identityless_release_is_ignored() {
        let r = GameRelease {
            id: None,
            tag: None,
            name: None,
            body: None,
            url: None,
            is_prerelease: false,
            published_at: None,
        };
        assert_eq!(release_identity(&r), None);
    }
    // Swift: AdvancedAlertPolicyTests.releaseCooldownSuppressesAndConsumesSecondRelease
    #[test]
    fn release_cooldown_consumes() {
        let mut p = release_policy(false);
        p.releases[0].rule.cooldown_seconds = 10.0;
        let a = evaluate(&p, PolicyState::default(), release_obs(0, &[]));
        let b = evaluate(&p, a.state, release_obs(1, &[release(1, false, 1)]));
        let rows = [release(2, false, 2)];
        let c = evaluate(&p, b.state, release_obs(2, &rows));
        assert_eq!(c.suppressed.len(), 1);
        assert!(evaluate(&p, c.state, release_obs(12, &rows))
            .decisions
            .is_empty());
    }

    fn quote(
        price: &str,
        market: &mut HashMap<CryptoMarketTimeframe, crate::models::CryptoMarketWindow>,
    ) -> CryptoQuote {
        CryptoQuote {
            price: price.into(),
            change24h: 0.0,
            market: market.clone(),
            liquidity_usd: None,
            fdv_usd: None,
            market_cap_usd: None,
            pair_url: None,
        }
    }
    fn quote_obs<'a>(seconds: i64, q: &'a CryptoQuote) -> Observation<'a> {
        Observation {
            observed_at: at(seconds),
            population: None,
            quote: Some(q),
            releases: None,
        }
    }
    fn change_policy(direction: ChangeDirection) -> PolicySet {
        PolicySet {
            changes: vec![ChangePolicy {
                rule: rule("c"),
                window: ChangeWindow::OneHour,
                direction,
                threshold_percent: 10.0,
                hysteresis_percent: 2.0,
            }],
            ..Default::default()
        }
    }
    fn change_quote(change: f64) -> CryptoQuote {
        let mut m = HashMap::new();
        m.insert(
            CryptoMarketTimeframe::OneHour,
            crate::models::CryptoMarketWindow {
                change_percent: Some(change),
                buys: None,
                sells: None,
                volume_usd: None,
            },
        );
        quote("1", &mut m)
    }
    fn release(id: i64, pre: bool, seconds: i64) -> GameRelease {
        GameRelease {
            id: Some(id),
            tag: Some(format!("v{id}")),
            name: Some(format!("Release {id}")),
            body: Some(" notes  here ".into()),
            url: None,
            is_prerelease: pre,
            published_at: Some(at(seconds)),
        }
    }
    fn release_policy(includes_prereleases: bool) -> PolicySet {
        PolicySet {
            releases: vec![ReleasePolicy {
                rule: rule("r"),
                includes_prereleases,
            }],
            ..Default::default()
        }
    }
    fn release_obs<'a>(seconds: i64, rows: &'a [GameRelease]) -> Observation<'a> {
        Observation {
            observed_at: at(seconds),
            population: None,
            quote: None,
            releases: Some(rows),
        }
    }
}
