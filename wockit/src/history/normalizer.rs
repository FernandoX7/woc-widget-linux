//! Retention and representation repair for history samples.

use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};

use super::HistorySample;

/// The normalized samples plus whether the input representation required repair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizationResult {
    pub samples: Vec<HistorySample>,
    pub repaired: bool,
}

/// Drop invalid/out-of-window samples, keep the first exact timestamp, and sort ascending.
pub fn normalize_samples(
    input: &[HistorySample],
    reference_date: DateTime<Utc>,
) -> Vec<HistorySample> {
    normalize_samples_with_limits(
        input,
        reference_date,
        Duration::seconds(crate::config::history::RETENTION_WINDOW as i64),
        Duration::seconds(crate::config::history::FUTURE_SAMPLE_TOLERANCE as i64),
    )
}

/// Normalize and report whether the frozen representation changed.
pub fn normalize_samples_with_report(
    input: &[HistorySample],
    reference_date: DateTime<Utc>,
) -> NormalizationResult {
    let samples = normalize_samples(input, reference_date);
    let repaired = samples != input;
    NormalizationResult { samples, repaired }
}

/// Merge a startup disk snapshot with observations recorded during loading.
///
/// Disk is deliberately ordered first, so it wins an exact-timestamp duplicate.
pub fn merge_startup_samples(
    disk: &[HistorySample],
    in_memory: &[HistorySample],
    reference_date: DateTime<Utc>,
) -> Vec<HistorySample> {
    let mut combined = Vec::with_capacity(disk.len() + in_memory.len());
    combined.extend_from_slice(disk);
    combined.extend_from_slice(in_memory);
    normalize_samples(&combined, reference_date)
}

fn normalize_samples_with_limits(
    input: &[HistorySample],
    reference_date: DateTime<Utc>,
    retention_window: Duration,
    future_tolerance: Duration,
) -> Vec<HistorySample> {
    let cutoff = reference_date - retention_window;
    let future_limit = reference_date + future_tolerance;
    let mut dates = HashSet::with_capacity(input.len());
    let mut output = Vec::with_capacity(input.len());

    for sample in input {
        if sample.count < 0
            || sample.date < cutoff
            || sample.date > future_limit
            || !dates.insert(sample.date)
        {
            continue;
        }
        output.push(sample.clone());
    }
    output.sort_by_key(|sample| sample.date);
    output
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).unwrap()
    }

    #[test]
    fn normalization_pipeline_is_covered_case_by_case() {
        let now = at(1_700_000_000);
        let cutoff = now - Duration::seconds(100);
        let future_limit = now + Duration::seconds(10);
        let duplicate_date = now - Duration::seconds(20);
        let first_duplicate = HistorySample::new(duplicate_date, 7);

        let result = normalize_samples_with_limits(
            &[
                HistorySample::new(cutoff - Duration::milliseconds(1), 1),
                first_duplicate.clone(),
                HistorySample::new(cutoff, 2),
                HistorySample::new(duplicate_date, 99),
                HistorySample::new(now, -1),
                HistorySample::new(future_limit, 4),
                HistorySample::new(future_limit + Duration::milliseconds(1), 5),
            ],
            now,
            Duration::seconds(100),
            Duration::seconds(10),
        );

        assert_eq!(
            result,
            vec![
                HistorySample::new(cutoff, 2),
                first_duplicate,
                HistorySample::new(future_limit, 4),
            ]
        );
    }

    #[test]
    fn startup_merge_keeps_disk_duplicate_and_new_memory_samples() {
        let now = at(1_700_000_000);
        let duplicate = now - Duration::seconds(30);
        let result = merge_startup_samples(
            &[HistorySample::new(duplicate, 10)],
            &[
                HistorySample::new(duplicate, 99),
                HistorySample::new(now, 20),
            ],
            now,
        );

        assert_eq!(
            result,
            vec![
                HistorySample::new(duplicate, 10),
                HistorySample::new(now, 20),
            ]
        );
    }

    #[test]
    fn repair_report_detects_drops_deduplication_and_sorting() {
        let now = at(1_700_000_000);
        let clean = vec![HistorySample::new(now, 1)];
        assert!(!normalize_samples_with_report(&clean, now).repaired);

        let dirty = vec![HistorySample::new(now, 1), HistorySample::new(now, 2)];
        assert!(normalize_samples_with_report(&dirty, now).repaired);
    }
}
