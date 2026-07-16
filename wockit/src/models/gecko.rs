//! GeckoTerminal OHLCV wire model and the pure wire-to-domain candle mapping.
//!
//! Ported from Networking/GeckoTerminalService.swift (the non-transport parts; the HTTP
//! seam is Phase 3).

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::candle::Candle;
use super::chart_time::CandleInterval;

/// Top-level GeckoTerminal OHLCV response. A hard decode, like the Swift `[[Double]]`:
/// any missing key or wrong-typed row fails the whole response.
/// Swift parity: GeckoOHLCVResponse in Networking/GeckoTerminalService.swift.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GeckoOhlcvResponse {
    pub data: GeckoOhlcvData,
}

/// Swift parity: GeckoOHLCVResponse.Node.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GeckoOhlcvData {
    pub attributes: GeckoOhlcvAttributes,
}

/// Swift parity: GeckoOHLCVResponse.Attributes (wire key `ohlcv_list`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GeckoOhlcvAttributes {
    pub ohlcv_list: Vec<Vec<f64>>,
}

/// Pure wire-to-domain mapping (GeckoTerminal returns newest-first; charts want
/// oldest-first). Each row is `[unixSeconds, open, high, low, close, volume]`;
/// malformed/impossible rows are dropped. Duplicate timestamps occasionally occur in the
/// live feed; the first valid row in the API payload wins deterministically, then the
/// result is sorted oldest-first.
/// Swift parity: GeckoTerminalService.candles(from:).
pub fn candles_from(response: &GeckoOhlcvResponse) -> Vec<Candle> {
    let mut seen: Vec<(u64, Candle)> = Vec::new();
    for row in &response.data.attributes.ohlcv_list {
        if row.len() < 5 {
            continue;
        }
        let raw_timestamp = row[0];
        // The Swift validity check runs on the raw row value: a NaN or nonpositive
        // source timestamp is invalid, and a chrono DateTime cannot represent it, so it
        // is rejected before a Candle is ever constructed.
        if !raw_timestamp.is_finite() || raw_timestamp <= 0.0 {
            continue;
        }
        let Some(date) = datetime_from_unix_seconds(raw_timestamp) else {
            continue;
        };
        let volume = if row.len() > 5 { row[5] } else { 0.0 };
        let candle = Candle::new(date, row[1], row[2], row[3], row[4], volume);
        if !candle.is_valid() {
            continue;
        }
        // Dedupe by the EXACT raw timestamp value. Bit equality matches value equality
        // here because NaN and +/-0.0 were already filtered out above.
        if seen
            .iter()
            .any(|(bits, _)| *bits == raw_timestamp.to_bits())
        {
            continue;
        }
        seen.push((raw_timestamp.to_bits(), candle));
    }
    let mut candles: Vec<Candle> = seen.into_iter().map(|(_, candle)| candle).collect();
    candles.sort_by_key(|candle| candle.date);
    candles
}

// Microsecond quantization: Swift keeps the raw Double in Date(timeIntervalSince1970:).
// The live feed emits whole-second timestamps, so micros preserve every realistic value
// exactly; dedupe already keys on the raw f64 bits, unaffected by this rounding.
fn datetime_from_unix_seconds(seconds: f64) -> Option<DateTime<Utc>> {
    let micros = (seconds * 1_000_000.0).round();
    if micros < i64::MIN as f64 || micros > i64::MAX as f64 {
        return None;
    }
    DateTime::from_timestamp_micros(micros as i64)
}

/// GeckoTerminal OHLCV endpoint: `{base}/{network}/pools/{pool}/ohlcv/{timeframe}` with
/// `aggregate`, `limit`, `currency=usd` in exactly that order. The pool address is
/// case-sensitive and passes through verbatim. `include_empty_intervals` is DELIBERATELY
/// omitted: filling no-trade gaps reintroduces the degenerate flat zero-volume candles
/// that switching to real OHLCV was meant to avoid (see [`Candle`]).
/// Swift parity: GeckoTerminalService.endpoint(interval:count:). The Phase 3 service
/// passes the `config::api` values for `base`/`network`/`pool`.
pub fn ohlcv_endpoint(
    base: &str,
    network: &str,
    pool: &str,
    interval: CandleInterval,
    count: u32,
) -> String {
    format!(
        "{base}/{network}/pools/{pool}/ohlcv/{timeframe}?aggregate={aggregate}&limit={count}&currency=usd",
        timeframe = interval.timeframe(),
        aggregate = interval.aggregate(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candles(json: &str) -> Vec<Candle> {
        let response: GeckoOhlcvResponse = serde_json::from_str(json).unwrap();
        candles_from(&response)
    }

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    // Swift parity: DecodingErrorTests.swift > GeckoTerminalServiceTests >
    // decodesAndSortsCandlesOldestFirst
    #[test]
    fn decodes_and_sorts_candles_oldest_first() {
        // GeckoTerminal returns newest-first; the mapping must flip to oldest-first.
        let json = r#"{"data":{"attributes":{"ohlcv_list":[[200,1.2,1.5,1.1,1.3,9],[100,1.0,1.1,0.9,1.05,8]]}}}"#;
        let candles = candles(json);
        assert_eq!(candles.len(), 2);
        assert_eq!(candles[0].date, at(100)); // oldest first
        assert_eq!(candles[1].date, at(200));
        assert_eq!(candles[0].open, 1.0);
        assert_eq!(candles[1].high, 1.5);
        assert!(candles[1].is_up()); // close 1.3 >= open 1.2
    }

    // Swift parity: DecodingErrorTests.swift > GeckoTerminalServiceTests > dropsMalformedRows
    #[test]
    fn drops_malformed_rows() {
        let json =
            r#"{"data":{"attributes":{"ohlcv_list":[[100,1.0,1.1,0.9],[200,1.2,1.5,1.1,1.3,9]]}}}"#;
        let candles = candles(json);
        assert_eq!(candles.len(), 1); // the 4-element row is dropped, the valid one kept
        assert_eq!(candles[0].close, 1.3);
    }

    // Swift parity: DecodingErrorTests.swift > GeckoTerminalServiceTests >
    // dropsInvalidOHLCAndDeduplicatesTimestampsDeterministically
    #[test]
    fn drops_invalid_ohlc_and_deduplicates_timestamps_deterministically() {
        let json = r#"{"data":{"attributes":{"ohlcv_list":[[100,1.0,1.2,0.9,1.1,8],[100,2.0,2.2,1.9,2.1,9],[200,1.0,0.8,0.9,1.1,4],[300,0,1,1,1,1]]}}}"#;
        let candles = candles(json);
        assert_eq!(candles.len(), 1);
        assert_eq!(candles[0].date, at(100));
        assert_eq!(candles[0].open, 1.0); // first valid duplicate wins
        assert_eq!(candles[0].volume, 8.0);
    }

    // Swift parity: DecodingErrorTests.swift > GeckoTerminalServiceTests >
    // buildsCaseSensitivePoolEndpointWithAggregateAndLimit
    #[test]
    fn builds_case_sensitive_pool_endpoint_with_aggregate_and_limit() {
        let url = ohlcv_endpoint(
            "https://b/networks",
            "solana",
            "PoOl",
            CandleInterval::FifteenMin,
            42,
        );
        // 15m -> minute timeframe, pool case preserved.
        assert!(url.contains("/solana/pools/PoOl/ohlcv/minute"));
        assert!(url.contains("aggregate=15"));
        assert!(url.contains("limit=42"));
        assert!(url.contains("currency=usd"));
    }

    // Swift parity: DecodingErrorTests.swift > GeckoTerminalServiceTests >
    // hourTimeframeForLongCandles
    #[test]
    fn hour_timeframe_for_long_candles() {
        let url = ohlcv_endpoint(
            "https://b/networks",
            "solana",
            "P",
            CandleInterval::FourHour,
            10,
        );
        assert!(url.contains("ohlcv/hour"));
        assert!(url.contains("aggregate=4"));
    }
}
