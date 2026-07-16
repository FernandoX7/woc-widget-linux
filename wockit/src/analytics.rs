//! Pure player-history analytics and gap segmentation.
//!
//! This is a direct port of WoCKit's `HistoryAnalytics` and `HistorySegmentation`.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, FixedOffset, Utc};

use crate::config::history;
use crate::history::HistorySample;
use crate::models::{ChartInterval, ChartRange};

#[derive(Clone, Debug, PartialEq)]
pub struct RealmRhythm {
    pub sample_count: usize,
    pub observed_duration: f64,
    pub coverage_fraction: f64,
    pub current_percentile: Option<i64>,
}

pub struct HistoryAnalytics<'a> {
    samples: &'a [HistorySample],
    now: DateTime<Utc>,
}

impl<'a> HistoryAnalytics<'a> {
    pub const fn new(samples: &'a [HistorySample], now: DateTime<Utc>) -> Self {
        Self { samples, now }
    }

    /// Samples inside the requested range, bucketed by their rounded mean.
    pub fn series(&self, range: ChartRange, interval: ChartInterval) -> Vec<HistorySample> {
        let start = self.now.timestamp() as f64 - range.seconds() as f64;
        let bucket =
            (interval.seconds() as f64).max(range.seconds() as f64 / history::CHART_MAX_POINTS);
        let mut buckets: BTreeMap<i64, (f64, usize)> = BTreeMap::new();

        for sample in self.samples {
            let timestamp = timestamp_seconds(sample.date);
            if timestamp >= start {
                let key = (timestamp / bucket).floor() * bucket;
                let entry = buckets.entry(key as i64).or_default();
                entry.0 += sample.count as f64;
                entry.1 += 1;
            }
        }

        buckets
            .into_iter()
            .filter_map(|(timestamp, (sum, count))| {
                DateTime::from_timestamp(timestamp, 0).map(|date| HistorySample {
                    date,
                    count: clamped_rounded_integer(sum / count as f64),
                })
            })
            .collect()
    }

    pub fn today_high(&self, fallback: i64, timezone: FixedOffset) -> i64 {
        let today = self.now.with_timezone(&timezone).date_naive();
        self.samples
            .iter()
            .filter(|sample| sample.date.with_timezone(&timezone).date_naive() == today)
            .map(|sample| sample.count)
            .max()
            .unwrap_or(fallback)
    }

    pub fn retention_cutoff(&self) -> DateTime<Utc> {
        self.now - chrono::Duration::seconds(history::RETENTION_WINDOW as i64)
    }

    pub fn average(series: &[HistorySample]) -> Option<i64> {
        (!series.is_empty()).then(|| {
            let total = series
                .iter()
                .fold(0.0, |sum, sample| sum + sample.count as f64);
            clamped_rounded_integer(total / series.len() as f64)
        })
    }

    pub fn change(
        &self,
        window_seconds: f64,
        current_count: i64,
        baseline_tolerance: f64,
    ) -> Option<i64> {
        if !window_seconds.is_finite()
            || window_seconds < 0.0
            || !baseline_tolerance.is_finite()
            || baseline_tolerance < 0.0
        {
            return None;
        }
        let target = timestamp_seconds(self.now) - window_seconds;
        let baseline = self.samples.iter().min_by(|left, right| {
            let left_distance = (timestamp_seconds(left.date) - target).abs();
            let right_distance = (timestamp_seconds(right.date) - target).abs();
            left_distance
                .total_cmp(&right_distance)
                .then_with(|| right.date.cmp(&left.date))
        })?;
        if (timestamp_seconds(baseline.date) - target).abs() > baseline_tolerance {
            return None;
        }
        Some(current_count.saturating_sub(baseline.count))
    }

    /// The overview's 30-minute comparison using the configured poll-aware tolerance.
    pub fn short_change(&self, current_count: i64, poll_seconds: f64) -> Option<i64> {
        let tolerance = history::SHORT_CHANGE_MINIMUM_TOLERANCE
            .max(poll_seconds * history::SHORT_CHANGE_POLL_TOLERANCE_MULTIPLIER);
        self.change(history::SHORT_CHANGE_WINDOW, current_count, tolerance)
    }

    pub fn realm_rhythm(
        &self,
        range: ChartRange,
        current_count: i64,
        expected_sample_interval: f64,
    ) -> RealmRhythm {
        let start = timestamp_seconds(self.now) - range.seconds() as f64;
        let now = timestamp_seconds(self.now);
        let mut ordered: Vec<_> = self
            .samples
            .iter()
            .filter(|sample| {
                let date = timestamp_seconds(sample.date);
                date >= start && date <= now && sample.count >= 0
            })
            .collect();
        ordered.sort_by_key(|sample| sample.date);
        let mut dates = HashSet::new();
        ordered.retain(|sample| dates.insert(sample.date));

        let cadence = if expected_sample_interval.is_finite() && expected_sample_interval > 0.0 {
            expected_sample_interval
        } else {
            0.0
        };
        let observed_duration = (ordered.len() as f64 * cadence).min(range.seconds() as f64);
        let coverage_fraction = (observed_duration / range.seconds() as f64).clamp(0.0, 1.0);
        let current_percentile = (ordered.len() >= history::RHYTHM_MINIMUM_SAMPLES).then(|| {
            let below = ordered
                .iter()
                .filter(|sample| sample.count < current_count)
                .count();
            let equal = ordered
                .iter()
                .filter(|sample| sample.count == current_count)
                .count();
            let rank = (below as f64 + equal as f64 * 0.5) / ordered.len() as f64;
            clamped_rounded_integer(rank * 100.0).clamp(0, 100)
        });

        RealmRhythm {
            sample_count: ordered.len(),
            observed_duration,
            coverage_fraction,
            current_percentile,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentedHistorySample {
    pub sample: HistorySample,
    pub segment: usize,
    /// True when this sample's segment contains no other point and should render as a dot.
    pub is_isolated: bool,
}

pub fn segment_history(
    samples: &[HistorySample],
    bucket_interval: f64,
    expected_sample_interval: f64,
) -> Vec<SegmentedHistorySample> {
    let mut ordered: Vec<_> = samples.iter().cloned().enumerate().collect();
    ordered.sort_by(|left, right| left.1.date.cmp(&right.1.date).then(left.0.cmp(&right.0)));
    let bucket = if bucket_interval.is_finite() {
        bucket_interval.max(0.0)
    } else {
        0.0
    };
    let sampling = if expected_sample_interval.is_finite() {
        expected_sample_interval.max(0.0)
    } else {
        0.0
    };
    let threshold = bucket.max(sampling) * history::CHART_GAP_MULTIPLIER;
    let mut segment = 0;
    let mut previous = None;
    let mut result = Vec::with_capacity(ordered.len());
    for (_, sample) in ordered {
        if previous.is_some_and(|date| timestamp_seconds(sample.date) - date > threshold) {
            segment += 1;
        }
        previous = Some(timestamp_seconds(sample.date));
        result.push(SegmentedHistorySample {
            sample,
            segment,
            is_isolated: false,
        });
    }
    let mut sizes = HashMap::new();
    for point in &result {
        *sizes.entry(point.segment).or_insert(0usize) += 1;
    }
    for point in &mut result {
        point.is_isolated = sizes[&point.segment] == 1;
    }
    result
}

/// Fraction of selected chart windows containing at least one observation, weighted for
/// partial buckets at the range edges.
pub fn window_coverage(
    samples: &[HistorySample],
    now: DateTime<Utc>,
    range: ChartRange,
    interval: ChartInterval,
) -> f64 {
    let width = interval.seconds() as f64;
    let range_seconds = range.seconds() as f64;
    if width <= 0.0 || range_seconds <= 0.0 {
        return 0.0;
    }
    let start = timestamp_seconds(now) - range_seconds;
    let bucket_ids: HashSet<i64> = samples
        .iter()
        .filter_map(|sample| {
            let raw = (timestamp_seconds(sample.date) / width).floor();
            (raw.is_finite() && raw >= i64::MIN as f64 && raw < i64::MAX as f64)
                .then_some(raw as i64)
        })
        .collect();
    let covered = bucket_ids.into_iter().fold(0.0, |total, id| {
        let bucket_start = id as f64 * width;
        let bucket_end = bucket_start + width;
        total + ((now.timestamp() as f64).min(bucket_end) - start.max(bucket_start)).max(0.0)
    });
    (covered / range_seconds).clamp(0.0, 1.0)
}

fn timestamp_seconds(date: DateTime<Utc>) -> f64 {
    date.timestamp() as f64 + date.timestamp_subsec_nanos() as f64 / 1_000_000_000.0
}

fn clamped_rounded_integer(value: f64) -> i64 {
    if !value.is_finite() {
        return if value.is_sign_negative() {
            i64::MIN
        } else {
            i64::MAX
        };
    }
    let rounded = value.round();
    if rounded >= i64::MAX as f64 {
        i64::MAX
    } else if rounded <= i64::MIN as f64 {
        i64::MIN
    } else {
        rounded as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }
    fn sample(offset: i64, count: i64) -> HistorySample {
        HistorySample::new(now() + chrono::Duration::seconds(offset), count)
    }

    #[test]
    fn empty_history_yields_no_series() {
        assert!(HistoryAnalytics::new(&[], now())
            .series(ChartRange::Day, ChartInterval::OneMin)
            .is_empty());
    }

    #[test]
    fn distinct_minute_buckets_are_sorted_ascending() {
        let samples = [sample(0, 10), sample(-60, 20), sample(-120, 30)];
        let series = HistoryAnalytics::new(&samples, now())
            .series(ChartRange::OneHour, ChartInterval::OneMin);
        assert_eq!(
            series.iter().map(|s| s.count).collect::<Vec<_>>(),
            [30, 20, 10]
        );
    }

    #[test]
    fn bucket_averages_its_samples() {
        let samples = [sample(0, 10), sample(-1, 20)];
        let series = HistoryAnalytics::new(&samples, now())
            .series(ChartRange::OneHour, ChartInterval::OneMin);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].count, 15);
    }

    #[test]
    fn window_excludes_samples_older_than_range() {
        let samples = [sample(0, 5), sample(-7200, 99)];
        let series = HistoryAnalytics::new(&samples, now())
            .series(ChartRange::OneHour, ChartInterval::OneMin);
        assert_eq!(series.iter().map(|s| s.count).collect::<Vec<_>>(), [5]);
    }

    #[test]
    fn never_exceeds_point_cap_and_downsamples() {
        let samples: Vec<_> = (0..=7 * 24 * 60)
            .map(|i| sample(-(7 * 24 * 3600) + i * 60, i % 50))
            .collect();
        assert!(samples.len() > 10_000);
        let series =
            HistoryAnalytics::new(&samples, now()).series(ChartRange::Week, ChartInterval::OneMin);
        assert!(series.len() <= 301);
        assert!(series.len() >= 290);
    }

    #[test]
    fn downsample_ceiling_holds_across_ranges_intervals_and_epoch_alignments() {
        for range in ChartRange::ALL_CASES {
            for interval in ChartInterval::ALL_CASES {
                for shift in [0, 1, 17, 59, 137] {
                    let shifted_now = now() + chrono::Duration::seconds(shift);
                    let samples: Vec<_> = (0..=range.seconds())
                        .step_by(10)
                        .map(|offset| {
                            HistorySample::new(
                                shifted_now - chrono::Duration::seconds(offset),
                                offset,
                            )
                        })
                        .collect();
                    let points = HistoryAnalytics::new(&samples, shifted_now)
                        .series(range, interval)
                        .len();
                    // Exact Swift parity: range/300 is the minimum bucket width, buckets
                    // are epoch-aligned, and both range endpoints are inclusive. Two
                    // partial edge buckets therefore permit an effective ceiling of 301.
                    assert!(
                        points <= 301,
                        "{range:?} {interval:?} shift {shift}: {points}"
                    );
                }
            }
        }
    }

    #[test]
    fn today_high_uses_todays_samples_else_fallback() {
        let utc = FixedOffset::east_opt(0).unwrap();
        let samples = [sample(0, 7), sample(-30, 12)];
        assert_eq!(
            HistoryAnalytics::new(&samples, now()).today_high(99, utc),
            12
        );
        let yesterday = [sample(-24 * 3600, 50)];
        assert_eq!(
            HistoryAnalytics::new(&yesterday, now()).today_high(99, utc),
            99
        );
    }

    #[test]
    fn retention_cutoff_is_seven_days_back() {
        assert_eq!(
            HistoryAnalytics::new(&[], now()).retention_cutoff(),
            sample(-7 * 24 * 3600, 0).date
        );
    }

    #[test]
    fn average_rounds_mean_and_is_none_when_empty() {
        assert_eq!(HistoryAnalytics::average(&[]), None);
        assert_eq!(
            HistoryAnalytics::average(&[sample(0, 10), sample(1, 15)]),
            Some(13)
        );
    }

    #[test]
    fn averages_extreme_counts_without_overflowing() {
        let samples = [sample(0, i64::MAX), sample(1, i64::MAX)];
        assert_eq!(HistoryAnalytics::average(&samples), Some(i64::MAX));
        assert_eq!(
            HistoryAnalytics::new(&samples, now())
                .series(ChartRange::OneHour, ChartInterval::OneMin)[0]
                .count,
            i64::MAX
        );
    }

    #[test]
    fn short_change_requires_a_baseline_near_the_requested_window() {
        let samples = [sample(-6 * 3600, 10), sample(-30 * 60 + 20, 80)];
        assert_eq!(
            HistoryAnalytics::new(&samples, now()).change(1800.0, 95, 60.0),
            Some(15)
        );
        assert_eq!(
            HistoryAnalytics::new(&samples[..1], now()).change(1800.0, 95, 60.0),
            None
        );
        assert_eq!(
            HistoryAnalytics::new(&samples, now()).short_change(95, 30.0),
            Some(15)
        );
    }

    #[test]
    fn short_change_clamps_arithmetic_overflow() {
        let baseline = [sample(-1800, i64::MAX)];
        assert_eq!(
            HistoryAnalytics::new(&baseline, now()).change(1800.0, i64::MIN, 1.0),
            Some(i64::MIN)
        );
    }

    #[test]
    fn realm_rhythm_reports_coverage_and_waits_for_enough_samples() {
        let sparse: Vec<_> = (0..10).map(|i| sample(-i * 60, 20 + i)).collect();
        let rhythm =
            HistoryAnalytics::new(&sparse, now()).realm_rhythm(ChartRange::OneHour, 25, 60.0);
        assert_eq!(rhythm.sample_count, 10);
        assert_eq!(rhythm.observed_duration, 600.0);
        assert!((rhythm.coverage_fraction - 1.0 / 6.0).abs() < 0.000_001);
        assert_eq!(rhythm.current_percentile, None);
    }

    #[test]
    fn realm_rhythm_uses_midpoint_percentile_and_caps_coverage() {
        let flat: Vec<_> = (0..40).map(|i| sample(-i * 60, 42)).collect();
        let rhythm =
            HistoryAnalytics::new(&flat, now()).realm_rhythm(ChartRange::OneHour, 42, 120.0);
        assert_eq!(rhythm.coverage_fraction, 1.0);
        assert_eq!(
            rhythm.observed_duration,
            ChartRange::OneHour.seconds() as f64
        );
        assert_eq!(rhythm.current_percentile, Some(50));
    }

    #[test]
    fn realm_rhythm_enables_percentile_at_exactly_thirty_samples() {
        let samples: Vec<_> = (0..30).map(|i| sample(-i * 60, i)).collect();
        let analytics = HistoryAnalytics::new(&samples, now());
        assert_eq!(
            analytics
                .realm_rhythm(ChartRange::OneHour, 15, 60.0)
                .current_percentile,
            Some(52)
        );
        assert_eq!(
            HistoryAnalytics::new(&samples[..29], now())
                .realm_rhythm(ChartRange::OneHour, 15, 60.0)
                .current_percentile,
            None
        );
    }

    #[test]
    fn segmentation_breaks_sparse_outages_and_preserves_regular_cadence() {
        let regular: Vec<_> = (0..4).map(|i| sample(i * 60, i)).collect();
        assert_eq!(
            segment_history(&regular, 60.0, 60.0)
                .iter()
                .map(|p| p.segment)
                .collect::<Vec<_>>(),
            [0, 0, 0, 0]
        );
        let sparse = [regular[0].clone(), sample(30 * 60, 9)];
        let result = segment_history(&sparse, 60.0, 60.0);
        assert_eq!(result.iter().map(|p| p.segment).collect::<Vec<_>>(), [0, 1]);
        assert!(result.iter().all(|p| p.is_isolated));
    }

    #[test]
    fn segmentation_sorts_input_and_uses_known_polling_cadence() {
        let a = sample(0, 1);
        let b = sample(5 * 60, 2);
        let duplicate = sample(5 * 60, 3);
        let c = sample(20 * 60, 4);
        let result = segment_history(
            &[c.clone(), duplicate.clone(), a.clone(), b.clone()],
            60.0,
            5.0 * 60.0,
        );
        // The upstream Swift test reverses the equal-date pair, contradicting the
        // implementation's explicit original-offset tie-breaker. Preserve the implemented
        // deterministic contract: equal timestamps retain their input order.
        assert_eq!(
            result.iter().map(|p| p.sample.date).collect::<Vec<_>>(),
            [a.date, duplicate.date, b.date, c.date]
        );
        assert_eq!(
            result.iter().map(|p| p.segment).collect::<Vec<_>>(),
            [0, 0, 0, 1]
        );
    }

    #[test]
    fn segmentation_splits_only_above_the_exact_gap_threshold() {
        let at_threshold = [sample(0, 1), sample(105, 2)];
        assert_eq!(
            segment_history(&at_threshold, 60.0, 60.0)
                .iter()
                .map(|point| point.segment)
                .collect::<Vec<_>>(),
            [0, 0]
        );
        let above_threshold = [sample(0, 1), sample(106, 2)];
        assert_eq!(
            segment_history(&above_threshold, 60.0, 60.0)
                .iter()
                .map(|point| point.segment)
                .collect::<Vec<_>>(),
            [0, 1]
        );
    }

    #[test]
    fn segmentation_invariants_hold_across_cadences_and_gaps() {
        for cadence in [0.0, 10.0, 60.0, 300.0] {
            for gap in [0, 1, 17, 105, 106, 525, 526, 1_800] {
                let input = [sample(gap, 3), sample(0, 1), sample(0, 2)];
                let result = segment_history(&input, 60.0, cadence);
                assert!(result
                    .windows(2)
                    .all(|pair| pair[0].sample.date <= pair[1].sample.date));
                assert!(result
                    .windows(2)
                    .all(|pair| pair[1].segment - pair[0].segment <= 1));
                for point in &result {
                    let segment_size = result
                        .iter()
                        .filter(|candidate| candidate.segment == point.segment)
                        .count();
                    assert_eq!(point.is_isolated, segment_size == 1);
                }
            }
        }
    }

    #[test]
    fn chart_window_coverage_weights_partial_edge_buckets() {
        let points = [sample(-3599, 1), sample(-1, 2)];
        let coverage = window_coverage(&points, now(), ChartRange::OneHour, ChartInterval::FiveMin);
        // `now` is 200 seconds into its five-minute bucket, so the two edge
        // windows overlap the range by 100 + 200 seconds.
        assert!((coverage - 300.0 / 3600.0).abs() < 0.000_001);
    }
}
