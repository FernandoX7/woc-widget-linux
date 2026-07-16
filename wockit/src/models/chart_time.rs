//! Chart timing vocabulary ported from Model/ChartTime.swift.

use crate::config::history::CHART_MAX_POINTS;
use crate::strings;

/// Player-chart bucket width. The discriminant is the bucket's duration in seconds (the
/// Swift `rawValue`, also the persisted key).
/// Swift parity: ChartInterval in Model/ChartTime.swift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChartInterval {
    OneMin = 60,
    FiveMin = 300,
    FifteenMin = 900,
    ThirtyMin = 1_800,
    OneHour = 3_600,
}

impl ChartInterval {
    /// Declaration order, mirroring Swift `CaseIterable.allCases`.
    pub const ALL_CASES: [ChartInterval; 5] = [
        ChartInterval::OneMin,
        ChartInterval::FiveMin,
        ChartInterval::FifteenMin,
        ChartInterval::ThirtyMin,
        ChartInterval::OneHour,
    ];

    pub fn seconds(self) -> i64 {
        self as i64
    }

    pub fn label(self) -> String {
        strings::compact_duration(self.seconds())
    }

    /// Smallest named resolution that stays within the chart's point budget for a full
    /// range. Choosing from the labels the UI can actually explain avoids claiming an
    /// opaque 72-second or 33.6-minute average when automatic downsampling is active.
    /// Swift parity: ChartInterval.automatic(for:) in Model/ChartTime.swift.
    pub fn automatic(range: ChartRange) -> ChartInterval {
        Self::ALL_CASES
            .into_iter()
            .find(|interval| range.seconds() as f64 / interval.seconds() as f64 <= CHART_MAX_POINTS)
            .unwrap_or(ChartInterval::OneHour)
    }
}

/// Candle width for the $WOC chart. Independent of the player chart's [`ChartInterval`]
/// so the two charts never share state, and constrained to the timeframes GeckoTerminal
/// actually serves (minute 1/5/15, hour 1/4) - a button can't request a candle the API
/// can't produce. The discriminant is the bar's duration in seconds (the persisted key);
/// `label` resolves through the shared localized compact-duration vocabulary.
/// Swift parity: CandleInterval in Model/ChartTime.swift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CandleInterval {
    OneMin = 60,
    FiveMin = 300,
    FifteenMin = 900,
    OneHour = 3_600,
    FourHour = 14_400,
}

impl CandleInterval {
    /// Declaration order, mirroring Swift `CaseIterable.allCases`.
    pub const ALL_CASES: [CandleInterval; 5] = [
        CandleInterval::OneMin,
        CandleInterval::FiveMin,
        CandleInterval::FifteenMin,
        CandleInterval::OneHour,
        CandleInterval::FourHour,
    ];

    pub fn seconds(self) -> i64 {
        self as i64
    }

    /// GeckoTerminal `ohlcv/{timeframe}` path segment.
    pub fn timeframe(self) -> &'static str {
        match self {
            CandleInterval::OneMin | CandleInterval::FiveMin | CandleInterval::FifteenMin => {
                "minute"
            }
            CandleInterval::OneHour | CandleInterval::FourHour => "hour",
        }
    }

    /// GeckoTerminal `aggregate` query value (bars per `timeframe` unit).
    pub fn aggregate(self) -> i64 {
        match self {
            CandleInterval::OneMin => 1,
            CandleInterval::FiveMin => 5,
            CandleInterval::FifteenMin => 15,
            CandleInterval::OneHour => 1,
            CandleInterval::FourHour => 4,
        }
    }

    pub fn label(self) -> String {
        strings::compact_duration(self.seconds())
    }
}

/// How far back the chart looks. Combined with [`ChartInterval`] (the bucket size).
/// Swift parity: ChartRange in Model/ChartTime.swift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChartRange {
    OneHour = 3_600,
    SixHours = 21_600,
    Day = 86_400,
    Week = 604_800,
}

impl ChartRange {
    /// Declaration order, mirroring Swift `CaseIterable.allCases`.
    pub const ALL_CASES: [ChartRange; 4] = [
        ChartRange::OneHour,
        ChartRange::SixHours,
        ChartRange::Day,
        ChartRange::Week,
    ];

    pub fn seconds(self) -> i64 {
        self as i64
    }

    pub fn label(self) -> String {
        strings::compact_duration(self.seconds())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Swift parity: FormattingTests.swift > CompactDurationTests >
    // domainIntervalsShareLocalizedCompactLabels (the ChartInterval/CandleInterval/
    // ChartRange assertions; PollInterval and AlertCooldownOption live in other modules).
    #[test]
    fn domain_intervals_share_localized_compact_labels() {
        assert_eq!(ChartInterval::ThirtyMin.label(), "30m");
        assert_eq!(CandleInterval::FourHour.label(), "4h");
        assert_eq!(ChartRange::Week.label(), "7d");
    }

    // No direct Swift test case; spot checks derived from the
    // ChartInterval.automatic(for:) rule in Model/ChartTime.swift (first declared case
    // keeping range/interval <= 300 points, else OneHour).
    #[test]
    fn automatic_picks_first_interval_within_the_point_budget() {
        // 3600/60 = 60 points <= 300.
        assert_eq!(
            ChartInterval::automatic(ChartRange::OneHour),
            ChartInterval::OneMin
        );
        // 21600/60 = 360 > 300; 21600/300 = 72 <= 300.
        assert_eq!(
            ChartInterval::automatic(ChartRange::SixHours),
            ChartInterval::FiveMin
        );
        // 86400/300 = 288 <= 300.
        assert_eq!(
            ChartInterval::automatic(ChartRange::Day),
            ChartInterval::FiveMin
        );
        // Week only fits at the hour bucket: 604800/3600 = 168 <= 300.
        assert_eq!(
            ChartInterval::automatic(ChartRange::Week),
            ChartInterval::OneHour
        );
    }
}
