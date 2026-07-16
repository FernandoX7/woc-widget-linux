//! Separately persisted local all-time peak state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::HistorySample;

/// Config representation for the record that outlives the seven-day history window.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPeak {
    #[serde(rename = "allTimePeak")]
    pub count: i64,
    #[serde(rename = "allTimePeakDate")]
    pub date: Option<DateTime<Utc>>,
}

impl LocalPeak {
    /// Repair persisted peak state (negative peaks are empty; empty peaks have no date).
    pub fn from_persisted(count: i64, date: Option<DateTime<Utc>>) -> Self {
        if count > 0 {
            Self { count, date }
        } else {
            Self::default()
        }
    }

    /// Reconcile upward from retained history. Existing records never move down.
    pub fn reconcile(&mut self, samples: &[HistorySample]) -> bool {
        let Some(high) = samples.iter().max_by_key(|sample| sample.count) else {
            return false;
        };
        if high.count <= self.count {
            return false;
        }
        self.count = high.count;
        self.date = Some(high.date);
        true
    }

    /// Clear both separately persisted peak values with local history.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn sample(seconds: i64, count: i64) -> HistorySample {
        HistorySample::new(Utc.timestamp_opt(seconds, 0).unwrap(), count)
    }

    #[test]
    fn reconciles_only_upward_and_uses_the_high_date() {
        let mut peak = LocalPeak::from_persisted(5, None);
        assert!(peak.reconcile(&[sample(1_700_000_000, 9), sample(1_700_000_060, 50)]));
        assert_eq!(
            peak,
            LocalPeak::from_persisted(50, Some(sample(1_700_000_060, 0).date))
        );

        assert!(!peak.reconcile(&[sample(1_700_000_120, 10)]));
        assert_eq!(peak.count, 50);
    }

    #[test]
    fn negative_persisted_peak_repairs_empty_and_clear_removes_date() {
        let date = sample(1_700_000_000, 1).date;
        assert_eq!(
            LocalPeak::from_persisted(-20, Some(date)),
            LocalPeak::default()
        );

        let mut peak = LocalPeak::from_persisted(50, Some(date));
        peak.clear();
        assert_eq!(peak, LocalPeak::default());
    }
}
