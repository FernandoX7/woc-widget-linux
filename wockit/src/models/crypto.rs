//! DEX Screener wire models and the `CryptoQuote` domain value.
//!
//! Ported from Networking/CryptoService.swift. The wire structs are public here (unlike
//! the file-private Swift originals) because the Phase 3 service crate-consumes them;
//! decode leniency is the whole point: every ancillary field uses Swift `try?` semantics
//! while `priceUsd` and `priceChange` hard-fail the pair decode.

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use url::Url;

use super::decode;

/// A DexScreener rolling-window timeframe. The wire keys are `m5`/`h1`/`h6`/`h24`.
/// Swift parity: CryptoMarketTimeframe in Networking/CryptoService.swift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CryptoMarketTimeframe {
    FiveMinutes,
    OneHour,
    SixHours,
    TwentyFourHours,
}

impl CryptoMarketTimeframe {
    /// Declaration order, mirroring Swift `CaseIterable.allCases` (m5, h1, h6, h24).
    pub const ALL_CASES: [CryptoMarketTimeframe; 4] = [
        CryptoMarketTimeframe::FiveMinutes,
        CryptoMarketTimeframe::OneHour,
        CryptoMarketTimeframe::SixHours,
        CryptoMarketTimeframe::TwentyFourHours,
    ];

    /// The DexScreener JSON key (Swift `rawValue`).
    pub fn wire_key(self) -> &'static str {
        match self {
            CryptoMarketTimeframe::FiveMinutes => "m5",
            CryptoMarketTimeframe::OneHour => "h1",
            CryptoMarketTimeframe::SixHours => "h6",
            CryptoMarketTimeframe::TwentyFourHours => "h24",
        }
    }
}

/// A DexScreener activity window. Every ancillary value is optional because young or
/// thinly traded pairs do not always receive a complete set of rolling-window statistics.
/// Swift parity: CryptoMarketWindow in Networking/CryptoService.swift.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CryptoMarketWindow {
    pub change_percent: Option<f64>,
    pub buys: Option<i64>,
    pub sells: Option<i64>,
    pub volume_usd: Option<f64>,
}

impl CryptoMarketWindow {
    /// `None` when the window carries no transaction data at all; a lone side counts as
    /// the total. Swift's `addingReportingOverflow` clamps ANY overflow — in either
    /// direction — to `Int.max`, so the exact twin is `checked_add` with an `i64::MAX`
    /// fallback (`saturating_add` would return `i64::MIN` on negative overflow).
    pub fn transaction_count(&self) -> Option<i64> {
        if self.buys.is_none() && self.sells.is_none() {
            return None;
        }
        Some(
            self.buys
                .unwrap_or(0)
                .checked_add(self.sells.unwrap_or(0))
                .unwrap_or(i64::MAX),
        )
    }
}

/// Domain value the store consumes. `price` and `change24h` deliberately preserve the
/// original API used by the status store; the remaining fields let richer market UI be
/// added without another endpoint or request.
/// Swift parity: CryptoQuote in Networking/CryptoService.swift.
#[derive(Debug, Clone, PartialEq)]
pub struct CryptoQuote {
    /// The exact wire string the UI renders (e.g. "0.0005594"). NEVER parsed to a float.
    pub price: String,
    pub change24h: f64,
    pub market: HashMap<CryptoMarketTimeframe, CryptoMarketWindow>,
    pub liquidity_usd: Option<f64>,
    pub fdv_usd: Option<f64>,
    pub market_cap_usd: Option<f64>,
    pub pair_url: Option<Url>,
}

impl CryptoQuote {
    pub fn metrics(&self, timeframe: CryptoMarketTimeframe) -> Option<&CryptoMarketWindow> {
        self.market.get(&timeframe)
    }

    /// The quote-construction half of Swift `CryptoService.fetchQuote`. Returns `None`
    /// when the pair carries no numeric 24-hour change - the required-change24h
    /// invariant (Swift throws `FetchError.decode` there); the Phase 3 service maps
    /// `None` to its decode error and supplies `pair_url` via
    /// `config::api::validated_market_url(pair.url)` (the dexscreener.com allowlist).
    pub fn from_pair(pair: &DexPair, pair_url: Option<Url>) -> Option<CryptoQuote> {
        let change24h = pair
            .price_change
            .get(CryptoMarketTimeframe::TwentyFourHours)
            .copied()?;
        let market = CryptoMarketTimeframe::ALL_CASES
            .into_iter()
            .map(|timeframe| {
                let transactions = pair
                    .transactions
                    .as_ref()
                    .and_then(|windows| windows.get(timeframe));
                (
                    timeframe,
                    CryptoMarketWindow {
                        change_percent: pair.price_change.get(timeframe).copied(),
                        buys: transactions.and_then(|t| t.buys),
                        sells: transactions.and_then(|t| t.sells),
                        volume_usd: pair
                            .volume
                            .as_ref()
                            .and_then(|windows| windows.get(timeframe))
                            .copied(),
                    },
                )
            })
            .collect();

        Some(CryptoQuote {
            price: pair.price_usd.clone(),
            change24h,
            market,
            liquidity_usd: pair.liquidity.as_ref().and_then(|l| l.usd),
            fdv_usd: pair.fdv,
            market_cap_usd: pair.market_cap,
            pair_url,
        })
    }
}

// MARK equivalent: DexScreener wire shape.

/// Top-level DexScreener response. `pairs` is the documented shape; `pair` remains a
/// compatibility fallback because the live endpoint currently emits both and older
/// fixtures/servers may emit one.
/// Swift parity: DexScreenerResponse in Networking/CryptoService.swift.
#[derive(Debug, Clone, PartialEq)]
pub struct DexScreenerResponse {
    pub pairs: Vec<DexPair>,
    pub pair: Option<DexPair>,
}

impl<'de> Deserialize<'de> for DexScreenerResponse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;
        // Swift: (try? [DexPair]) ?? [] - the try? wraps the WHOLE array, so one
        // undecodable element empties `pairs` rather than dropping just that row.
        let pairs = map
            .get("pairs")
            .and_then(|v| serde_json::from_value::<Vec<DexPair>>(v.clone()).ok())
            .unwrap_or_default();
        let pair = decode::lenient(map.get("pair"));
        Ok(Self { pairs, pair })
    }
}

impl DexScreenerResponse {
    /// Prefer the documented array even while the compatibility alias is present. Within
    /// a multi-pair response, the most completely verified candidate wins rather than
    /// API order; the alias is used only when it matches the configured identity itself.
    /// Swift parity: DexScreenerResponse.selectPair(chainID:pairAddress:tokenAddress:).
    pub fn select_pair(
        &self,
        chain_id: &str,
        pair_address: &str,
        token_address: &str,
    ) -> Option<&DexPair> {
        if let Some(official) = self.best_match(chain_id, pair_address, token_address) {
            return Some(official);
        }
        let pair = self.pair.as_ref()?;
        pair.identity_match_score(chain_id, pair_address, token_address)?;
        Some(pair)
    }

    fn best_match(
        &self,
        chain_id: &str,
        pair_address: &str,
        token_address: &str,
    ) -> Option<&DexPair> {
        self.pairs
            .iter()
            .filter_map(|candidate| {
                candidate
                    .identity_match_score(chain_id, pair_address, token_address)
                    .map(|score| (candidate, score))
            })
            // Swift `max(by: <)` keeps the FIRST of equal maxima; the strict `>` here
            // mirrors that tie behavior.
            .reduce(|best, candidate| {
                if candidate.1 > best.1 {
                    candidate
                } else {
                    best
                }
            })
            .map(|(pair, _)| pair)
    }
}

/// One DexScreener pair. Only `priceUsd` (a raw string, never parsed) and `priceChange`
/// are required; everything else is lenient.
/// Swift parity: DexPair in Networking/CryptoService.swift.
#[derive(Debug, Clone, PartialEq)]
pub struct DexPair {
    pub chain_id: Option<String>,
    pub pair_address: Option<String>,
    pub base_token: Option<DexToken>,
    pub quote_token: Option<DexToken>,
    /// Kept as the exact wire string end to end - never parsed into a float.
    pub price_usd: String,
    pub price_change: DexRollingValues<f64>,
    /// Wire key `txns`.
    pub transactions: Option<DexRollingValues<DexTransactionCount>>,
    pub volume: Option<DexRollingValues<f64>>,
    pub liquidity: Option<DexLiquidity>,
    pub fdv: Option<f64>,
    pub market_cap: Option<f64>,
    pub url: Option<String>,
}

impl<'de> Deserialize<'de> for DexPair {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;
        // priceUsd is required as a string - a numeric price is a schema failure, same
        // as the Swift `try c.decode(String.self, forKey: .priceUsd)`.
        let price_usd = match map.get("priceUsd") {
            Some(Value::String(raw)) => raw.clone(),
            _ => return Err(D::Error::custom("priceUsd must be a string")),
        };
        // priceChange is required as an object; the timeframes inside stay lenient.
        let price_change = map
            .get("priceChange")
            .cloned()
            .ok_or_else(|| D::Error::custom("priceChange is required"))
            .and_then(|raw| {
                serde_json::from_value::<DexRollingValues<f64>>(raw)
                    .map_err(|error| D::Error::custom(format!("priceChange: {error}")))
            })?;
        Ok(Self {
            chain_id: decode::lenient(map.get("chainId")),
            pair_address: decode::lenient(map.get("pairAddress")),
            base_token: decode::lenient(map.get("baseToken")),
            quote_token: decode::lenient(map.get("quoteToken")),
            price_usd,
            price_change,
            transactions: decode::lenient(map.get("txns")),
            volume: decode::lenient(map.get("volume")),
            liquidity: decode::lenient(map.get("liquidity")),
            fdv: decode::lenient(map.get("fdv")),
            market_cap: decode::lenient(map.get("marketCap")),
            url: decode::lenient(map.get("url")),
        })
    }
}

impl DexPair {
    /// Returns `None` for a definite identity mismatch. Missing legacy fields are
    /// tolerated because they provide no evidence either way; present identifiers add
    /// confidence to selection. The chain matches case-insensitively; the pair address
    /// must equal the canonical mixed-case value EXACTLY - never case-normalized.
    /// Swift parity: DexPair.identityMatchScore(chainID:pairAddress:tokenAddress:).
    pub fn identity_match_score(
        &self,
        chain_id: &str,
        pair_address: &str,
        token_address: &str,
    ) -> Option<i32> {
        let mut score = 0;

        if let Some(candidate_chain) = &self.chain_id {
            if candidate_chain.to_lowercase() != chain_id.to_lowercase() {
                return None;
            }
            score += 1;
        }
        if let Some(candidate_pair) = &self.pair_address {
            if candidate_pair != pair_address {
                return None;
            }
            score += 4;
        }

        let base_address = self.base_token.as_ref().map(|token| token.address.as_str());
        let quote_address = self
            .quote_token
            .as_ref()
            .map(|token| token.address.as_str());
        if base_address == Some(token_address) || quote_address == Some(token_address) {
            score += 2;
        } else if base_address.is_some() && quote_address.is_some() {
            // Both sides are known, so the configured token is definitively absent.
            return None;
        }

        Some(score)
    }
}

/// Swift parity: DexToken in Networking/CryptoService.swift (`address` required).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DexToken {
    pub address: String,
}

/// Swift parity: DexTransactionCount in Networking/CryptoService.swift (both lenient).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DexTransactionCount {
    pub buys: Option<i64>,
    pub sells: Option<i64>,
}

impl<'de> Deserialize<'de> for DexTransactionCount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;
        Ok(Self {
            buys: decode::lenient(map.get("buys")),
            sells: decode::lenient(map.get("sells")),
        })
    }
}

/// One value per rolling window; each field is lenient but the container itself must be
/// a JSON object. Swift parity: DexRollingValues in Networking/CryptoService.swift.
#[derive(Debug, Clone, PartialEq)]
pub struct DexRollingValues<V> {
    pub m5: Option<V>,
    pub h1: Option<V>,
    pub h6: Option<V>,
    pub h24: Option<V>,
}

impl<V> DexRollingValues<V> {
    /// Swift's `subscript(_ timeframe:)`.
    pub fn get(&self, timeframe: CryptoMarketTimeframe) -> Option<&V> {
        match timeframe {
            CryptoMarketTimeframe::FiveMinutes => self.m5.as_ref(),
            CryptoMarketTimeframe::OneHour => self.h1.as_ref(),
            CryptoMarketTimeframe::SixHours => self.h6.as_ref(),
            CryptoMarketTimeframe::TwentyFourHours => self.h24.as_ref(),
        }
    }
}

impl<'de, V: DeserializeOwned> Deserialize<'de> for DexRollingValues<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;
        Ok(Self {
            m5: decode::lenient(map.get("m5")),
            h1: decode::lenient(map.get("h1")),
            h6: decode::lenient(map.get("h6")),
            h24: decode::lenient(map.get("h24")),
        })
    }
}

/// Swift parity: DexLiquidity in Networking/CryptoService.swift (`usd` lenient).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DexLiquidity {
    pub usd: Option<f64>,
}

impl<'de> Deserialize<'de> for DexLiquidity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;
        Ok(Self {
            usd: decode::lenient(map.get("usd")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The Swift service defaults used by the parity fixtures (AppConfig.API values the
    // injected CryptoService falls back to). Passed explicitly here because the service
    // seam is Phase 3.
    const CHAIN: &str = "solana";
    const PAIR: &str = "5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p";
    const TOKEN: &str = "3WjLscH2JsXLEFJZRA9z8ti8yRGxWGKbqymPd7UicRth";

    fn response(json: &str) -> DexScreenerResponse {
        serde_json::from_str(json).unwrap()
    }

    fn quote(json: &str) -> Option<CryptoQuote> {
        let response = response(json);
        let pair = response.select_pair(CHAIN, PAIR, TOKEN)?;
        CryptoQuote::from_pair(pair, None)
    }

    // Swift parity: DecodingErrorTests.swift > CryptoServiceTests > decodesValidPair
    #[test]
    fn decodes_valid_pair() {
        let q = quote(r#"{"pair":{"priceUsd":"0.0005594","priceChange":{"h24":47.59}}}"#).unwrap();
        assert_eq!(q.price, "0.0005594"); // the exact wire string, never a parsed float
        assert_eq!(q.change24h, 47.59);
    }

    // Swift parity: DecodingErrorTests.swift > CryptoServiceTests > throwsDecodeWhenPairMissing
    // (select_pair yields None; the Phase 3 service maps that None to FetchError::Decode).
    #[test]
    fn select_pair_yields_none_when_pair_missing() {
        let r = response(r#"{"pair":null}"#);
        assert_eq!(r.pair, None);
        assert!(r.pairs.is_empty());
        assert!(r.select_pair(CHAIN, PAIR, TOKEN).is_none());
    }

    // Swift parity: ExtendedCryptoServiceTests.swift > ExtendedCryptoServiceTests >
    // decodesEveryUsefulDexScreenerMarketField (decode + selection + quote mapping only;
    // the Swift case goes through fetchQuote, whose transport seam is Phase 3, and its
    // pairURL assertion needs config::api::validated_market_url).
    #[test]
    fn decodes_every_useful_dex_screener_market_field() {
        let json = r#"
        {
          "pairs": [{
            "chainId": "solana",
            "pairAddress": "5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p",
            "baseToken": {
              "address": "3WjLscH2JsXLEFJZRA9z8ti8yRGxWGKbqymPd7UicRth",
              "name": "World Of Claudecraft",
              "symbol": "WOC"
            },
            "quoteToken": {
              "address": "So11111111111111111111111111111111111111112",
              "name": "Wrapped SOL",
              "symbol": "SOL"
            },
            "priceUsd": "0.0005666",
            "priceChange": {"m5": 0.23, "h1": -2.71, "h6": 15.76, "h24": 23.05},
            "txns": {
              "m5": {"buys": 1, "sells": 2},
              "h1": {"buys": 30, "sells": 22},
              "h6": {"buys": 314, "sells": 260},
              "h24": {"buys": 532, "sells": 516}
            },
            "volume": {"m5": 77.18, "h1": 6984.42, "h6": 50082.73, "h24": 84007.14},
            "liquidity": {"usd": 71161.77, "base": 62482164, "quote": 459.8154},
            "fdv": 566648,
            "marketCap": 566647.5,
            "url": "https://dexscreener.com/solana/pair-id"
          }]
        }"#;

        let q = quote(json).unwrap();
        assert_eq!(q.price, "0.0005666");
        assert_eq!(q.change24h, 23.05); // original status-store-facing API remains intact
        assert_eq!(
            q.metrics(CryptoMarketTimeframe::FiveMinutes)
                .and_then(|w| w.change_percent),
            Some(0.23)
        );
        assert_eq!(
            q.metrics(CryptoMarketTimeframe::OneHour)
                .and_then(|w| w.buys),
            Some(30)
        );
        assert_eq!(
            q.metrics(CryptoMarketTimeframe::SixHours)
                .and_then(|w| w.sells),
            Some(260)
        );
        assert_eq!(
            q.metrics(CryptoMarketTimeframe::TwentyFourHours)
                .and_then(|w| w.transaction_count()),
            Some(1_048)
        );
        assert_eq!(
            q.metrics(CryptoMarketTimeframe::TwentyFourHours)
                .and_then(|w| w.volume_usd),
            Some(84_007.14)
        );
        assert_eq!(q.liquidity_usd, Some(71_161.77));
        assert_eq!(q.fdv_usd, Some(566_648.0));
        assert_eq!(q.market_cap_usd, Some(566_647.5));
    }

    // Swift parity: ExtendedCryptoServiceTests.swift > ExtendedCryptoServiceTests >
    // documentedPairsArrayTakesPrecedenceOverCompatibilityAlias (selection layer only).
    #[test]
    fn documented_pairs_array_takes_precedence_over_compatibility_alias() {
        let json = r#"
        {
          "pairs": [{
            "chainId": "solana",
            "pairAddress": "5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p",
            "baseToken": {"address": "3WjLscH2JsXLEFJZRA9z8ti8yRGxWGKbqymPd7UicRth"},
            "quoteToken": {"address": "So11111111111111111111111111111111111111112"},
            "priceUsd": "1.25",
            "priceChange": {"h24": 5}
          }],
          "pair": {
            "priceUsd": "999",
            "priceChange": {"h24": 99}
          }
        }"#;

        let q = quote(json).unwrap();
        assert_eq!(q.price, "1.25");
        assert_eq!(q.change24h, 5.0);
    }

    // Swift parity: ExtendedCryptoServiceTests.swift > ExtendedCryptoServiceTests >
    // selectsConfiguredIdentityFromMultipleDocumentedPairs (selection layer only; also
    // exercises the case-insensitive chain match on "SOLANA").
    #[test]
    fn selects_configured_identity_from_multiple_documented_pairs() {
        let json = r#"
        {
          "pairs": [
            {
              "chainId": "solana",
              "pairAddress": "WrongPool11111111111111111111111111111111111",
              "baseToken": {"address": "WrongToken1111111111111111111111111111111111"},
              "quoteToken": {"address": "So11111111111111111111111111111111111111112"},
              "priceUsd": "900",
              "priceChange": {"h24": 90}
            },
            {
              "chainId": "SOLANA",
              "pairAddress": "5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p",
              "baseToken": {"address": "3WjLscH2JsXLEFJZRA9z8ti8yRGxWGKbqymPd7UicRth"},
              "quoteToken": {"address": "So11111111111111111111111111111111111111112"},
              "priceUsd": "0.75",
              "priceChange": {"h24": -4}
            }
          ]
        }"#;

        let q = quote(json).unwrap();
        assert_eq!(q.price, "0.75");
        assert_eq!(q.change24h, -4.0);
    }

    // Swift parity: ExtendedCryptoServiceTests.swift > ExtendedCryptoServiceTests >
    // rejectsDocumentedPairWithConflictingConfiguredIdentity (selection layer only; the
    // Phase 3 service maps the None to FetchError::Decode). The middle case is the
    // case-sensitive pair-address invariant; the wrong-pool address differs from the
    // canonical mixed-case one.
    #[test]
    fn rejects_documented_pair_with_conflicting_configured_identity() {
        let wrong_identities = [
            r#""chainId":"ethereum","pairAddress":"5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p","baseToken":{"address":"3WjLscH2JsXLEFJZRA9z8ti8yRGxWGKbqymPd7UicRth"},"quoteToken":{"address":"So11111111111111111111111111111111111111112"}"#,
            r#""chainId":"solana","pairAddress":"WrongPool11111111111111111111111111111111111","baseToken":{"address":"3WjLscH2JsXLEFJZRA9z8ti8yRGxWGKbqymPd7UicRth"},"quoteToken":{"address":"So11111111111111111111111111111111111111112"}"#,
            r#""chainId":"solana","pairAddress":"5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p","baseToken":{"address":"WrongToken1111111111111111111111111111111111"},"quoteToken":{"address":"So11111111111111111111111111111111111111112"}"#,
        ];

        for identity in wrong_identities {
            let json =
                format!(r#"{{"pairs":[{{{identity},"priceUsd":"1","priceChange":{{"h24":1}}}}]}}"#);
            assert!(response(&json).select_pair(CHAIN, PAIR, TOKEN).is_none());
        }
    }

    // Swift parity: ExtendedCryptoServiceTests.swift > ExtendedCryptoServiceTests >
    // fallsBackToMatchingCompatibilityPairWhenArrayHasNoMatch (selection layer only).
    #[test]
    fn falls_back_to_matching_compatibility_pair_when_array_has_no_match() {
        let json = r#"
        {
          "pairs": [{
            "chainId": "ethereum",
            "pairAddress": "WrongPool11111111111111111111111111111111111",
            "baseToken": {"address": "WrongToken1111111111111111111111111111111111"},
            "quoteToken": {"address": "OtherToken1111111111111111111111111111111111"},
            "priceUsd": "900",
            "priceChange": {"h24": 90}
          }],
          "pair": {
            "chainId": "solana",
            "pairAddress": "5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p",
            "baseToken": {"address": "3WjLscH2JsXLEFJZRA9z8ti8yRGxWGKbqymPd7UicRth"},
            "quoteToken": {"address": "So11111111111111111111111111111111111111112"},
            "priceUsd": "0.50",
            "priceChange": {"h24": 3}
          }
        }"#;

        let q = quote(json).unwrap();
        assert_eq!(q.price, "0.50");
        assert_eq!(q.change24h, 3.0);
    }

    // Swift parity: ExtendedCryptoServiceTests.swift > ExtendedCryptoServiceTests >
    // malformedAncillaryFieldsDoNotDiscardCoreQuote (decode + quote mapping only; the
    // Swift pairURL == nil assertion is the validated_market_url allowlist, Phase 3).
    #[test]
    fn malformed_ancillary_fields_do_not_discard_core_quote() {
        let json = r#"
        {
          "pair": {
            "priceUsd": "1.23",
            "priceChange": {"m5": "unknown", "h1": 2, "h24": 4.5},
            "txns": {"h1": {"buys": "many", "sells": 3}, "h24": false},
            "volume": {"h1": "unknown", "h24": 120},
            "liquidity": {"usd": "unknown"},
            "fdv": "unknown",
            "marketCap": null,
            "url": "not-a-canonical-web-url"
          }
        }"#;

        let q = quote(json).unwrap();
        assert_eq!(q.price, "1.23");
        assert_eq!(q.change24h, 4.5);
        assert_eq!(
            q.metrics(CryptoMarketTimeframe::FiveMinutes)
                .and_then(|w| w.change_percent),
            None
        );
        assert_eq!(
            q.metrics(CryptoMarketTimeframe::OneHour)
                .and_then(|w| w.buys),
            None
        );
        assert_eq!(
            q.metrics(CryptoMarketTimeframe::OneHour)
                .and_then(|w| w.sells),
            Some(3)
        );
        assert_eq!(
            q.metrics(CryptoMarketTimeframe::TwentyFourHours)
                .and_then(|w| w.volume_usd),
            Some(120.0)
        );
        assert_eq!(q.liquidity_usd, None);
        assert_eq!(q.fdv_usd, None);
        assert_eq!(q.market_cap_usd, None);
    }

    // Swift parity: ExtendedCryptoServiceTests.swift > ExtendedCryptoServiceTests >
    // transactionCountIsNilWhenTheWindowHasNoTransactionData
    #[test]
    fn transaction_count_is_none_when_the_window_has_no_transaction_data() {
        let absent = CryptoMarketWindow {
            change_percent: Some(2.0),
            buys: None,
            sells: None,
            volume_usd: Some(3.0),
        };
        let partial = CryptoMarketWindow {
            change_percent: None,
            buys: Some(4),
            sells: None,
            volume_usd: None,
        };
        let overflowing = CryptoMarketWindow {
            change_percent: None,
            buys: Some(i64::MAX),
            sells: Some(1),
            volume_usd: None,
        };
        // Swift addingReportingOverflow clamps overflow in EITHER direction to Int.max.
        let underflowing = CryptoMarketWindow {
            change_percent: None,
            buys: Some(i64::MIN),
            sells: Some(-1),
            volume_usd: None,
        };
        assert_eq!(absent.transaction_count(), None);
        assert_eq!(partial.transaction_count(), Some(4));
        assert_eq!(overflowing.transaction_count(), Some(i64::MAX));
        assert_eq!(underflowing.transaction_count(), Some(i64::MAX));
    }

    // No direct Swift test case; leniency edges required by the Swift decode structure
    // (DexPair in Networking/CryptoService.swift): a wrong-typed ancillary field stays
    // None while the pair decodes, a missing priceUsd fails the whole pair, and a
    // wrong-typed pairs array nukes to [] (whole-array try?, not per-element lossy).
    #[test]
    fn leniency_edges_match_the_swift_decode_structure() {
        // fdv wrong-typed -> None while the pair still decodes.
        let r = response(r#"{"pair":{"priceUsd":"1","priceChange":{"h24":1.0},"fdv":"abc"}}"#);
        let pair = r.pair.as_ref().unwrap();
        assert_eq!(pair.fdv, None);
        assert_eq!(pair.price_usd, "1");

        // priceUsd missing -> the DexPair decode fails -> lenient pair alias -> None.
        let r = response(r#"{"pair":{"priceChange":{"h24":1.0}}}"#);
        assert_eq!(r.pair, None);

        // priceUsd as a number is a schema failure, not silently stringified.
        let r = response(r#"{"pair":{"priceUsd":0.5,"priceChange":{"h24":1.0}}}"#);
        assert_eq!(r.pair, None);

        // Wrong-typed pairs value -> [].
        let r = response(r#"{"pairs":5,"pair":{"priceUsd":"1","priceChange":{"h24":1.0}}}"#);
        assert!(r.pairs.is_empty());

        // One undecodable element empties the WHOLE array (it is not lossy per-element).
        let r =
            response(r#"{"pairs":[{"priceUsd":"1","priceChange":{"h24":1.0}},{"broken":true}]}"#);
        assert!(r.pairs.is_empty());

        // from_pair requires the 24-hour change (required-change24h invariant).
        let r = response(r#"{"pair":{"priceUsd":"1","priceChange":{"h1":2.0}}}"#);
        let pair = r.select_pair(CHAIN, PAIR, TOKEN).unwrap();
        assert!(CryptoQuote::from_pair(pair, None).is_none());
    }
}
