//! Baseline-safe release alert reduction and monitor lifecycle policy.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::models::GameRelease;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAlertPayload {
    pub identity: String,
    pub tag: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub url: Option<url::Url>,
    pub is_prerelease: bool,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseAlertState {
    has_observation: bool,
    observed_identities: HashSet<String>,
}

impl ReleaseAlertState {
    /// Consumes one successful feed observation. The first observation only establishes a
    /// baseline; subsequent observations announce at most the newest eligible release.
    pub fn observe(&mut self, releases: &[GameRelease]) -> Option<ReleaseAlertPayload> {
        let identified: Vec<_> = releases
            .iter()
            .filter_map(|release| release_identity(release).map(|identity| (identity, release)))
            .collect();

        if !self.has_observation {
            self.has_observation = true;
            self.observed_identities
                .extend(identified.iter().map(|(identity, _)| identity.clone()));
            return None;
        }

        let eligible: Vec<_> = identified
            .iter()
            .enumerate()
            .filter(|(_, (identity, release))| {
                !self.observed_identities.contains(identity) && !release.is_prerelease
            })
            .collect();

        // Swift parity: reduceReleases consumes all identities, including prereleases and
        // releases not selected by newest-only delivery.
        self.observed_identities
            .extend(identified.iter().map(|(identity, _)| identity.clone()));

        eligible
            .into_iter()
            .max_by(|(left_index, (_, left)), (right_index, (_, right))| {
                left.published_at
                    .cmp(&right.published_at)
                    // Equal/missing dates preserve the first feed item, matching Swift's
                    // enumerated().max comparator.
                    .then_with(|| right_index.cmp(left_index))
            })
            .map(|(_, (identity, release))| payload(identity.clone(), release))
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Pure lifecycle seam for the release monitor. Scheduling and fetching stay with the caller.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseMonitorPolicy {
    started: bool,
    enabled: bool,
    in_flight: bool,
    releases: ReleaseAlertState,
}

impl ReleaseMonitorPolicy {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    /// Returns whether the caller should schedule and immediately perform a check.
    pub fn start(&mut self) -> bool {
        self.started = true;
        self.is_active()
    }

    /// Enabling while started requests an immediate check. Both transitions discard the old
    /// baseline so releases observed during a disabled period cannot replay.
    pub fn set_enabled(&mut self, enabled: bool) -> bool {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.releases.reset();
            if !enabled {
                self.in_flight = false;
            }
        }
        self.is_active()
    }

    pub fn is_active(&self) -> bool {
        self.started && self.enabled
    }

    /// Acquires the single-flight tick guard without reading a clock.
    pub fn begin_tick(&mut self) -> bool {
        if !self.enabled || self.in_flight {
            return false;
        }
        self.in_flight = true;
        true
    }

    /// A successful result mutates the baseline only if monitoring is still enabled.
    pub fn finish_success(&mut self, releases: &[GameRelease]) -> Option<ReleaseAlertPayload> {
        if !self.in_flight {
            return None;
        }
        self.in_flight = false;
        self.enabled
            .then(|| self.releases.observe(releases))
            .flatten()
    }

    /// Failures deliberately do not masquerade as an empty successful observation.
    pub fn finish_failure(&mut self) {
        self.in_flight = false;
    }
}

pub fn release_identity(release: &GameRelease) -> Option<String> {
    if let Some(id) = release.id {
        return Some(format!("id:{id}"));
    }
    if let Some(tag) = normalized_text(release.tag.as_deref()) {
        return Some(format!("tag:{}", tag.to_lowercase()));
    }
    if let Some(url) = &release.url {
        return Some(format!("url:{url}"));
    }
    if let Some(published_at) = release.published_at {
        let title = normalized_text(release.name.as_deref())
            .map(|value| value.to_lowercase())
            .unwrap_or_else(|| "untitled".into());
        let seconds = published_at.timestamp() as f64
            + f64::from(published_at.timestamp_subsec_nanos()) / 1e9;
        let timestamp = if seconds.fract() == 0.0 {
            format!("{seconds:.1}")
        } else {
            seconds.to_string()
        };
        return Some(format!("published:{timestamp}:{title}"));
    }
    normalized_text(release.name.as_deref()).map(|name| format!("name:{}", name.to_lowercase()))
}

fn payload(identity: String, release: &GameRelease) -> ReleaseAlertPayload {
    ReleaseAlertPayload {
        identity,
        tag: normalized_text(release.tag.as_deref()).map(str::to_owned),
        name: normalized_text(release.name.as_deref()).map(str::to_owned),
        summary: release_summary(release.body.as_deref()),
        url: release.url.clone(),
        is_prerelease: release.is_prerelease,
        published_at: release.published_at,
    }
}

fn release_summary(value: Option<&str>) -> Option<String> {
    let text = normalized_text(value)?;
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= 240 {
        return Some(collapsed);
    }
    let prefix: String = collapsed.chars().take(239).collect();
    Some(format!("{}…", prefix.trim()))
}

fn normalized_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn release(id: i64, tag: &str) -> GameRelease {
        GameRelease {
            id: Some(id),
            tag: Some(tag.into()),
            name: None,
            body: None,
            url: None,
            is_prerelease: false,
            published_at: None,
        }
    }

    // Swift source: ReleaseAlertMonitorTests.successfulTicksSeedThenNotifyOnlyForANewRelease
    #[test]
    fn successful_ticks_seed_then_deliver_newest_only() {
        let v1 = release(1, "v1");
        let mut v2 = release(2, "v2");
        v2.body = Some("  New\n dungeons  ".into());
        let mut state = ReleaseAlertState::default();
        assert_eq!(state.observe(std::slice::from_ref(&v1)), None);
        let result = state.observe(&[v2, v1]).unwrap();
        assert_eq!(result.identity, "id:2");
        assert_eq!(result.summary.as_deref(), Some("New dungeons"));
    }

    // Swift source: ReleaseAlertMonitorTests.failedTickDoesNotSeedOrMutateReleaseObservationState
    #[test]
    fn failed_tick_does_not_seed() {
        let mut policy = ReleaseMonitorPolicy::new(true);
        assert!(policy.begin_tick());
        policy.finish_failure();
        assert!(policy.begin_tick());
        assert_eq!(policy.finish_success(&[release(2, "v2")]), None);
        assert!(policy.begin_tick());
        assert_eq!(
            policy.finish_success(&[release(3, "v3")]).unwrap().identity,
            "id:3"
        );
    }

    // Swift source: ReleaseAlertMonitorTests.disabledMonitorNeitherSchedulesNorFetchesAndEnableStartsImmediately
    // and enabledMonitorSchedulesAndChecksImmediatelyOnStart
    #[test]
    fn lifecycle_activation_matches_start_and_enable() {
        let mut disabled = ReleaseMonitorPolicy::new(false);
        assert!(!disabled.start());
        assert!(!disabled.begin_tick());
        assert!(disabled.set_enabled(true));
        assert!(disabled.begin_tick());

        let mut enabled = ReleaseMonitorPolicy::new(true);
        assert!(enabled.start());
        assert!(enabled.is_active());
    }

    // Swift source: ReleaseAlertMonitorTests.reenableReseedsInsteadOfReplayingReleasesFromDisabledPeriod
    #[test]
    fn reenable_reseeds() {
        let mut policy = ReleaseMonitorPolicy::new(true);
        assert!(policy.begin_tick());
        assert_eq!(policy.finish_success(&[release(1, "v1")]), None);
        policy.set_enabled(false);
        policy.set_enabled(true);
        assert!(policy.begin_tick());
        assert_eq!(policy.finish_success(&[release(2, "v2")]), None);
        assert!(policy.begin_tick());
        assert_eq!(
            policy.finish_success(&[release(3, "v3")]).unwrap().identity,
            "id:3"
        );
    }

    // Swift source: AdvancedAlertPolicyTests release identity/newest/prerelease/summary cases.
    #[test]
    fn identities_precedence_prereleases_consumed_and_summary_is_bounded() {
        let mut item = release(9, " Tag ");
        item.url = Some(url::Url::parse("https://example.com/release").unwrap());
        item.published_at = Utc.timestamp_opt(123, 0).single();
        item.name = Some(" Name ".into());
        assert_eq!(release_identity(&item).as_deref(), Some("id:9"));

        item.id = None;
        assert_eq!(release_identity(&item).as_deref(), Some("tag:tag"));
        item.tag = None;
        assert_eq!(
            release_identity(&item).as_deref(),
            Some("url:https://example.com/release")
        );
        item.url = None;
        assert_eq!(
            release_identity(&item).as_deref(),
            Some("published:123.0:name")
        );
        item.published_at = Utc.timestamp_opt(-1, 500_000_000).single();
        assert_eq!(
            release_identity(&item).as_deref(),
            Some("published:-0.5:name")
        );

        let mut prerelease = release(10, "beta");
        prerelease.is_prerelease = true;
        let mut state = ReleaseAlertState::default();
        state.observe(&[]);
        assert_eq!(state.observe(std::slice::from_ref(&prerelease)), None);
        prerelease.is_prerelease = false;
        assert_eq!(state.observe(&[prerelease]), None);

        let mut long = release(11, "v11");
        long.body = Some("x".repeat(300));
        let payload = payload("id:11".into(), &long);
        assert_eq!(payload.summary.unwrap().chars().count(), 240);
    }
}
