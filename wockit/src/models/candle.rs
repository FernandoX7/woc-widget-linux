//! OHLC candle domain type for the $WOC price chart.

use chrono::{DateTime, Utc};

/// One OHLC candle for the $WOC price chart. Fetched ready-made from GeckoTerminal (a
/// real per-bar open/high/low/close), NOT synthesized from spot polls - synthesizing
/// candles from the slow DexScreener spot price produced degenerate dojis.
/// USD-denominated to match the `priceUsd` string rendered elsewhere.
///
/// Deliberately not serde: the Swift `Candle` is not Codable; candles are always rebuilt
/// from the wire (see [`super::candles_from`]).
/// Swift parity: Candle in Model/Models.swift.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candle {
    /// Bar open time.
    pub date: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Candle {
    /// Swift's memberwise init defaults `volume` to 0; Rust callers pass it explicitly
    /// (use `0.0` for volume-less bars).
    pub fn new(
        date: DateTime<Utc>,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Self {
        Self {
            date,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    pub fn is_up(&self) -> bool {
        self.close >= self.open
    }

    /// Domain validation applied both at the network boundary and again in the store so
    /// injected services cannot poison chart scaling with NaN/Inf, negative prices, or
    /// impossible OHLC.
    ///
    /// The Swift check also rejects NaN/nonpositive *source* timestamps; a chrono
    /// `DateTime` cannot represent those, so [`super::candles_from`] validates the raw
    /// row timestamp before a `Candle` is ever constructed. Here the timestamp derived
    /// from `date` is always finite; the `> 0` check still rejects epoch-or-earlier bars.
    pub fn is_valid(&self) -> bool {
        let timestamp = self.date.timestamp_micros() as f64 / 1_000_000.0;
        let shape_valid = timestamp.is_finite()
            && timestamp > 0.0
            && self.open.is_finite()
            && self.high.is_finite()
            && self.low.is_finite()
            && self.close.is_finite()
            && self.open > 0.0
            && self.high > 0.0
            && self.low > 0.0
            && self.close > 0.0
            && self.low <= self.open.min(self.close)
            && self.high >= self.open.max(self.close)
            && self.low <= self.high;
        shape_valid && self.volume.is_finite() && self.volume >= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    // No direct Swift test case for Candle.isValid in isolation; the rules are exercised
    // through GeckoTerminalServiceTests and StorePolicyTests.swift > StorePolicyTests >
    // candleNormalizationSortsFiltersAndKeepsFirstDuplicate (the invalid fixture there is
    // open 2 / high 1: high < max(open, close)). This mirrors those rule-by-rule.
    #[test]
    fn is_valid_ports_the_swift_domain_rules() {
        let valid = Candle::new(at(1_700_000_000), 1.0, 2.0, 0.5, 1.5, 10.0);
        assert!(valid.is_valid());
        assert!(valid.is_up());

        // high < max(open, close) - the StorePolicyTests invalid fixture.
        assert!(!Candle::new(at(1_700_000_000), 2.0, 1.0, 0.5, 1.5, 0.0).is_valid());
        // Nonpositive price.
        assert!(!Candle::new(at(1_700_000_000), 0.0, 1.0, 1.0, 1.0, 1.0).is_valid());
        // low > min(open, close).
        assert!(!Candle::new(at(1_700_000_000), 1.0, 2.0, 1.2, 1.1, 1.0).is_valid());
        // Non-finite price.
        assert!(!Candle::new(at(1_700_000_000), f64::NAN, 2.0, 0.5, 1.5, 1.0).is_valid());
        // Negative or non-finite volume.
        assert!(!Candle::new(at(1_700_000_000), 1.0, 2.0, 0.5, 1.5, -1.0).is_valid());
        assert!(!Candle::new(at(1_700_000_000), 1.0, 2.0, 0.5, 1.5, f64::INFINITY).is_valid());
        // Epoch-or-earlier bar open time.
        assert!(!Candle::new(at(0), 1.0, 2.0, 0.5, 1.5, 1.0).is_valid());
        assert!(!Candle::new(at(-10), 1.0, 2.0, 0.5, 1.5, 1.0).is_valid());

        // Down candle: close < open.
        assert!(!Candle::new(at(100), 1.5, 2.0, 0.5, 1.0, 1.0).is_up());
    }
}
