//! GeckoTerminal candle client.

use crate::config;
use crate::error::FetchError;
use crate::http::{fetch_decoded, HttpTransport, ReqwestTransport};
use crate::models::{candles_from, ohlcv_endpoint, Candle, CandleInterval, GeckoOhlcvResponse};

/// One-shot GeckoTerminal OHLCV reads. The service owns no timer or retry state.
#[derive(Clone, Debug)]
pub struct GeckoTerminalService<T = ReqwestTransport> {
    transport: T,
    base: String,
    network: String,
    pool: String,
}

impl Default for GeckoTerminalService<ReqwestTransport> {
    fn default() -> Self {
        Self::new()
    }
}

impl GeckoTerminalService<ReqwestTransport> {
    /// Live service configured for the canonical, case-sensitive $WOC pool.
    pub fn new() -> Self {
        Self::with_transport(
            ReqwestTransport::new(),
            config::api::GECKO_BASE,
            config::api::GECKO_NETWORK,
            config::api::GECKO_POOL,
        )
    }
}

impl<T: HttpTransport> GeckoTerminalService<T> {
    /// Injectable construction seam used by parity tests.
    pub fn with_transport(
        transport: T,
        base: impl Into<String>,
        network: impl Into<String>,
        pool: impl Into<String>,
    ) -> Self {
        Self {
            transport,
            base: base.into(),
            network: network.into(),
            pool: pool.into(),
        }
    }

    /// Fetch the configured dashboard candle count (60), oldest first.
    pub async fn fetch_candles(&self, interval: CandleInterval) -> Result<Vec<Candle>, FetchError> {
        self.fetch_candles_with_count(interval, config::crypto::CANDLE_COUNT as u32)
            .await
    }

    /// Fetch a caller-selected count. This mirrors the Swift injection seam.
    pub async fn fetch_candles_with_count(
        &self,
        interval: CandleInterval,
        count: u32,
    ) -> Result<Vec<Candle>, FetchError> {
        let endpoint = self.endpoint(interval, count);
        let decoded: GeckoOhlcvResponse = fetch_decoded(&self.transport, &endpoint).await?;
        Ok(candles_from(&decoded))
    }

    /// Build the exact GeckoTerminal request URL, preserving pool-address case.
    pub fn endpoint(&self, interval: CandleInterval, count: u32) -> url::Url {
        url::Url::parse(&ohlcv_endpoint(
            &self.base,
            &self.network,
            &self.pool,
            interval,
            count,
        ))
        .expect("GeckoTerminal service configuration must form a valid URL")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransportErrorKind;
    use crate::http::HttpResponse;

    struct FakeTransport {
        body: &'static str,
        status: u16,
    }

    impl HttpTransport for FakeTransport {
        async fn get(&self, _url: &url::Url) -> Result<HttpResponse, TransportErrorKind> {
            Ok(HttpResponse {
                status: self.status,
                body: self.body.as_bytes().to_vec(),
            })
        }
    }

    fn service(body: &'static str, status: u16) -> GeckoTerminalService<FakeTransport> {
        GeckoTerminalService::with_transport(
            FakeTransport { body, status },
            "https://example.invalid/networks",
            "solana",
            "PoOl",
        )
    }

    #[tokio::test]
    async fn fetches_validates_dedupes_and_sorts_candles() {
        let body = r#"{"data":{"attributes":{"ohlcv_list":[[200,1.2,1.5,1.1,1.3,9],[100,1,1.2,0.9,1.1,8],[100,2,2.2,1.9,2.1,9],[300,1,0.8,0.9,1.1,4],[400,0,1,1,1,1]]}}}"#;
        let candles = service(body, 200)
            .fetch_candles(CandleInterval::FiveMin)
            .await
            .unwrap();

        assert_eq!(candles.len(), 2);
        assert_eq!(candles[0].date.timestamp(), 100);
        assert_eq!(candles[0].open, 1.0); // first valid duplicate wins
        assert_eq!(candles[1].date.timestamp(), 200);
    }

    #[tokio::test]
    async fn empty_ohlcv_list_is_a_successful_empty_result() {
        let candles = service(r#"{"data":{"attributes":{"ohlcv_list":[]}}}"#, 200)
            .fetch_candles(CandleInterval::FiveMin)
            .await
            .unwrap();

        assert!(candles.is_empty());
    }

    #[tokio::test]
    async fn maps_non_200_through_shared_transport() {
        let error = service("{}", 503)
            .fetch_candles(CandleInterval::OneHour)
            .await
            .unwrap_err();
        assert_eq!(error, FetchError::Http(503));
    }

    #[tokio::test]
    async fn lowercased_pool_404_is_preserved_as_an_http_error() {
        let service = GeckoTerminalService::with_transport(
            FakeTransport {
                body: "{}",
                status: 404,
            },
            "https://example.invalid/networks",
            "solana",
            "pool",
        );

        assert!(service
            .endpoint(CandleInterval::OneHour, 60)
            .path()
            .contains("/pools/pool/ohlcv/hour"));
        let error = service
            .fetch_candles(CandleInterval::OneHour)
            .await
            .unwrap_err();
        assert_eq!(error, FetchError::Http(404));
    }

    #[test]
    fn endpoint_preserves_pool_case_and_maps_all_intervals() {
        let service = service("{}", 200);
        let cases = [
            (CandleInterval::OneMin, "minute", "1"),
            (CandleInterval::FiveMin, "minute", "5"),
            (CandleInterval::FifteenMin, "minute", "15"),
            (CandleInterval::OneHour, "hour", "1"),
            (CandleInterval::FourHour, "hour", "4"),
        ];
        for (interval, timeframe, aggregate) in cases {
            let url = service.endpoint(interval, 60);
            assert!(url
                .path()
                .contains(&format!("/pools/PoOl/ohlcv/{timeframe}")));
            let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            assert_eq!(query.get("aggregate").map(String::as_str), Some(aggregate));
            assert_eq!(query.get("limit").map(String::as_str), Some("60"));
            assert_eq!(query.get("currency").map(String::as_str), Some("usd"));
            assert!(!query.contains_key("include_empty_intervals"));
        }
    }
}
