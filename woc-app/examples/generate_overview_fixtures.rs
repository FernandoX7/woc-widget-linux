use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, Utc};
use serde::Serialize;
use wockit::analytics::{segment_history, window_coverage, HistoryAnalytics};
use wockit::history::HistorySample;
use wockit::models::{ChartInterval, ChartRange};

const REFERENCE: &str = "2026-07-11T18:00:00Z";
const RANGES: [(&str, ChartRange); 4] = [
    ("1h", ChartRange::OneHour),
    ("6h", ChartRange::SixHours),
    ("24h", ChartRange::Day),
    ("7d", ChartRange::Week),
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    chart_range_seconds: i64,
    chart_interval_seconds: i64,
    chart_interval_label: String,
    chart_points: Vec<Point>,
    chart_y_domain: Option<Domain>,
    window_coverage_percent: i64,
    short_change: Option<i64>,
    today_high: Option<i64>,
    local_record: i64,
    range_average: Option<i64>,
    rhythm: Rhythm,
}

#[derive(Serialize)]
struct Point {
    time: i64,
    value: Option<i64>,
    isolated: bool,
}

#[derive(Serialize)]
struct Domain {
    min: i64,
    max: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Rhythm {
    sample_count: usize,
    coverage_percent: i64,
    current_percentile: Option<i64>,
}

fn history(offset_seconds: i64) -> Vec<HistorySample> {
    let now = REFERENCE.parse::<DateTime<Utc>>().unwrap();
    (0..=360)
        .map(|index| {
            let minutes_ago = 360 - index;
            let base = 82.0
                + 14.0 * (f64::from(minutes_ago) / 30.0).sin()
                + 5.0 * (f64::from(minutes_ago) / 7.0).sin();
            let noise = ((minutes_ago * 17 + 11) % 7) - 3;
            HistorySample {
                date: now - chrono::Duration::minutes(i64::from(minutes_ago))
                    + chrono::Duration::seconds(offset_seconds),
                count: (base + f64::from(noise)).round().max(0.0) as i64,
            }
        })
        .collect()
}

fn scenario_history(name: &str) -> Vec<HistorySample> {
    let source = history(0);
    match name {
        "loading" | "empty-history" => Vec::new(),
        "cached-offline" => history(-7_200),
        "gap-history" => source[..8]
            .iter()
            .chain(std::iter::once(&source[120]))
            .chain(source[240..248].iter())
            .cloned()
            .collect(),
        "rhythm-29" => source[source.len() - 29..].to_vec(),
        "rhythm-30" => source[source.len() - 30..].to_vec(),
        _ => source,
    }
}

fn output(samples: &[HistorySample], current: Option<i64>, range: ChartRange) -> Output {
    let now = REFERENCE.parse::<DateTime<Utc>>().unwrap();
    let analytics = HistoryAnalytics::new(samples, now);
    let interval = ChartInterval::automatic(range);
    let series = analytics.series(range, interval);
    let segmented = segment_history(&series, interval.seconds() as f64, 60.0);
    let mut chart_points = Vec::with_capacity(segmented.len() * 2);
    let mut previous_segment = None;
    let mut previous_time = None;
    for point in segmented {
        if previous_segment.is_some_and(|segment| segment != point.segment) {
            chart_points.push(Point {
                time: previous_time.unwrap_or(point.sample.date.timestamp()) + 1,
                value: None,
                isolated: false,
            });
            chart_points.push(Point {
                time: point.sample.date.timestamp() - 1,
                value: None,
                isolated: false,
            });
        }
        previous_segment = Some(point.segment);
        previous_time = Some(point.sample.date.timestamp());
        chart_points.push(Point {
            time: point.sample.date.timestamp(),
            value: Some(point.sample.count),
            isolated: point.is_isolated,
        });
    }
    let chart_y_domain = series
        .iter()
        .map(|sample| sample.count)
        .min()
        .zip(series.iter().map(|sample| sample.count).max())
        .map(|(minimum, maximum)| Domain {
            min: minimum.saturating_sub(3).max(0),
            max: maximum.saturating_add(3),
        });
    let mut rhythm = analytics.realm_rhythm(range, current.unwrap_or_default(), 60.0);
    if current.is_none() {
        rhythm.current_percentile = None;
    }
    Output {
        chart_range_seconds: range.seconds(),
        chart_interval_seconds: interval.seconds(),
        chart_interval_label: interval.label(),
        chart_points,
        chart_y_domain,
        window_coverage_percent: (window_coverage(&series, now, range, interval) * 100.0).round()
            as i64,
        short_change: current.and_then(|count| analytics.short_change(count, 60.0)),
        today_high: current.map(|count| {
            analytics.today_high(count, FixedOffset::east_opt(0).expect("UTC offset"))
        }),
        local_record: samples.iter().map(|sample| sample.count).max().unwrap_or(0),
        range_average: HistoryAnalytics::average(&series),
        rhythm: Rhythm {
            sample_count: rhythm.sample_count,
            coverage_percent: (rhythm.coverage_fraction * 100.0).round() as i64,
            current_percentile: rhythm.current_percentile,
        },
    }
}

fn main() {
    let scenarios = [
        "live",
        "welcome",
        "loading",
        "cached-offline",
        "quote-only",
        "chart-only",
        "empty-history",
        "gap-history",
        "rhythm-29",
        "rhythm-30",
    ];
    let mut fixture = BTreeMap::new();
    for scenario in scenarios {
        let samples = scenario_history(scenario);
        let current =
            (scenario != "loading").then(|| history(0).last().expect("live history").count);
        let ranges = RANGES
            .into_iter()
            .map(|(label, range)| (label, output(&samples, current, range)))
            .collect::<BTreeMap<_, _>>();
        fixture.insert(scenario, ranges);
    }
    println!("{}", serde_json::to_string_pretty(&fixture).unwrap());
}
