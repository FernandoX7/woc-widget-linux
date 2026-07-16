//! DEX Screener quote service with configured-market identity validation.

use url::Url;

use crate::config;
use crate::error::FetchError;
use crate::http::{fetch_decoded, HttpTransport, ReqwestTransport};
use crate::models::{CryptoQuote, DexScreenerResponse};

/// Fetches a quote and rejects candidates that definitely conflict with the configured
/// chain, canonical pair address, or token address.
/// Swift parity: `CryptoService` in Networking/CryptoService.swift.
#[derive(Debug)]
pub struct CryptoService<T> {
    transport: T,
    endpoint: Url,
    expected_chain_id: String,
    expected_pair_address: String,
    expected_token_address: String,
}

impl<T> CryptoService<T> {
    pub fn new(transport: T, endpoint: Url) -> Self {
        Self::with_expected_identity(
            transport,
            endpoint,
            config::api::DEX_CHAIN,
            config::api::DEX_CANONICAL_PAIR_ADDRESS,
            config::api::DEX_TOKEN_ADDRESS,
        )
    }

    pub fn with_expected_identity(
        transport: T,
        endpoint: Url,
        expected_chain_id: impl Into<String>,
        expected_pair_address: impl Into<String>,
        expected_token_address: impl Into<String>,
    ) -> Self {
        Self {
            transport,
            endpoint,
            expected_chain_id: expected_chain_id.into(),
            expected_pair_address: expected_pair_address.into(),
            expected_token_address: expected_token_address.into(),
        }
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }
}

impl<T: HttpTransport> CryptoService<T> {
    pub async fn fetch_quote(&self) -> Result<CryptoQuote, FetchError> {
        let decoded: DexScreenerResponse = fetch_decoded(&self.transport, &self.endpoint).await?;
        let pair = decoded
            .select_pair(
                &self.expected_chain_id,
                &self.expected_pair_address,
                &self.expected_token_address,
            )
            .ok_or(FetchError::Decode)?;
        let pair_url = config::api::validated_market_url(pair.url.as_deref());
        CryptoQuote::from_pair(pair, pair_url).ok_or(FetchError::Decode)
    }
}

impl CryptoService<ReqwestTransport> {
    /// Live service using the configured DEX Screener pair endpoint and identity.
    pub fn live() -> Self {
        Self::new(ReqwestTransport::new(), config::api::crypto_url())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransportErrorKind;
    use crate::http::HttpResponse;
    use crate::models::CryptoMarketTimeframe;

    struct FakeHttp {
        body: Vec<u8>,
        status: u16,
    }

    impl HttpTransport for FakeHttp {
        async fn get(&self, _url: &Url) -> Result<HttpResponse, TransportErrorKind> {
            Ok(HttpResponse {
                status: self.status,
                body: self.body.clone(),
            })
        }
    }

    fn service(json: &str) -> CryptoService<FakeHttp> {
        CryptoService::new(
            FakeHttp {
                body: json.as_bytes().to_vec(),
                status: 200,
            },
            Url::parse("https://example.invalid/pair").unwrap(),
        )
    }

    fn identity() -> &'static str {
        r#""chainId":"solana","pairAddress":"5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p","baseToken":{"address":"3WjLscH2JsXLEFJZRA9z8ti8yRGxWGKbqymPd7UicRth"},"quoteToken":{"address":"So11111111111111111111111111111111111111112"}"#
    }

    #[tokio::test]
    async fn decodes_core_and_ancillary_market_fields() {
        let json = format!(
            r#"{{"pairs":[{{{},"priceUsd":"0.0005666","priceChange":{{"m5":0.23,"h24":23.05}},"txns":{{"h24":{{"buys":532,"sells":516}}}},"volume":{{"h24":84007.14}},"liquidity":{{"usd":71161.77}},"fdv":566648,"marketCap":566647.5,"url":"https://dexscreener.com/solana/pair-id"}}]}}"#,
            identity()
        );
        let quote = service(&json).fetch_quote().await.unwrap();
        assert_eq!(quote.price, "0.0005666");
        assert_eq!(quote.change24h, 23.05);
        assert_eq!(
            quote
                .metrics(CryptoMarketTimeframe::TwentyFourHours)
                .unwrap()
                .transaction_count(),
            Some(1048)
        );
        assert_eq!(quote.liquidity_usd, Some(71161.77));
        assert_eq!(
            quote.pair_url.as_ref().map(Url::as_str),
            Some("https://dexscreener.com/solana/pair-id")
        );
    }

    #[tokio::test]
    async fn rejects_each_definite_identity_mismatch() {
        let wrong = [
            r#""chainId":"ethereum","pairAddress":"5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p","baseToken":{"address":"3WjLscH2JsXLEFJZRA9z8ti8yRGxWGKbqymPd7UicRth"},"quoteToken":{"address":"So11111111111111111111111111111111111111112"}"#,
            r#""chainId":"solana","pairAddress":"5we9yjzpeqxcyl4jn9khjtsr48xzyh47xtar9kg3wy1p","baseToken":{"address":"3WjLscH2JsXLEFJZRA9z8ti8yRGxWGKbqymPd7UicRth"},"quoteToken":{"address":"So11111111111111111111111111111111111111112"}"#,
            r#""chainId":"solana","pairAddress":"5wE9YJzPeQxCYL4jN9KhjTSR48Xzyh47xTAR9kg3wy1p","baseToken":{"address":"wrong"},"quoteToken":{"address":"also-wrong"}"#,
        ];
        for identity in wrong {
            let json = format!(
                r#"{{"pairs":[{{{},"priceUsd":"1","priceChange":{{"h24":1}}}}]}}"#,
                identity
            );
            assert_eq!(
                service(&json).fetch_quote().await.unwrap_err(),
                FetchError::Decode
            );
        }
    }

    #[tokio::test]
    async fn documented_array_precedes_alias_and_best_identity_match_wins() {
        let json = format!(
            r#"{{"pairs":[{{"priceUsd":"2","priceChange":{{"h24":2}}}},{{{},"priceUsd":"1","priceChange":{{"h24":1}}}}],"pair":{{"priceUsd":"9","priceChange":{{"h24":9}}}}}}"#,
            identity()
        );
        assert_eq!(service(&json).fetch_quote().await.unwrap().price, "1");
    }

    #[tokio::test]
    async fn market_url_requires_https_and_dexscreener_host() {
        for candidate in [
            "http://dexscreener.com/solana/pair",
            "https://example.com/pair",
        ] {
            let json = format!(
                r#"{{"pair":{{"priceUsd":"1","priceChange":{{"h24":1}},"url":"{}"}}}}"#,
                candidate
            );
            assert_eq!(service(&json).fetch_quote().await.unwrap().pair_url, None);
        }
    }

    #[tokio::test]
    async fn non_200_is_mapped_by_shared_fetch_boundary() {
        let result = CryptoService::new(
            FakeHttp {
                body: b"{}".to_vec(),
                status: 503,
            },
            Url::parse("https://example.invalid/pair").unwrap(),
        )
        .fetch_quote()
        .await;
        assert_eq!(result.unwrap_err(), FetchError::Http(503));
    }
}
