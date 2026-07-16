//! First-party realm-status service.

use url::Url;

use crate::error::FetchError;
use crate::http::{fetch_decoded, HttpTransport, ReqwestTransport};
use crate::models::StatusResponse;

/// Fetches the player count and realm status from a configurable endpoint.
/// Swift parity: `StatusService` in Networking/StatusService.swift.
#[derive(Debug)]
pub struct StatusService<T> {
    transport: T,
    endpoint: Url,
}

impl<T> StatusService<T> {
    pub fn new(transport: T, endpoint: Url) -> Self {
        Self {
            transport,
            endpoint,
        }
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }
}

impl<T: HttpTransport> StatusService<T> {
    pub async fn fetch_status(&self) -> Result<StatusResponse, FetchError> {
        fetch_decoded(&self.transport, &self.endpoint).await
    }
}

impl StatusService<ReqwestTransport> {
    /// Live service using the configured first-party status endpoint.
    pub fn live() -> Self {
        Self::new(
            ReqwestTransport::new(),
            Url::parse(crate::config::api::STATUS_URL)
                .expect("static status endpoint must be a valid URL"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransportErrorKind;
    use crate::http::HttpResponse;

    struct FakeHttp {
        body: &'static [u8],
    }

    impl HttpTransport for FakeHttp {
        async fn get(&self, _url: &Url) -> Result<HttpResponse, TransportErrorKind> {
            Ok(HttpResponse {
                status: 200,
                body: self.body.to_vec(),
            })
        }
    }

    fn service(body: &'static [u8]) -> StatusService<FakeHttp> {
        StatusService::new(
            FakeHttp { body },
            Url::parse("https://example.invalid/status").unwrap(),
        )
    }

    #[tokio::test]
    async fn requires_players_online_and_defaults_ok_true() {
        let status = service(br#"{"players_online":42}"#)
            .fetch_status()
            .await
            .unwrap();
        assert_eq!(status.players_online, 42);
        assert!(status.ok);
        assert_eq!(status.names, None);

        assert_eq!(
            service(br#"{"ok":true}"#).fetch_status().await.unwrap_err(),
            FetchError::Decode
        );
    }

    #[tokio::test]
    async fn preserves_absent_roster_vs_explicit_empty_roster() {
        let absent = service(br#"{"players_online":3}"#)
            .fetch_status()
            .await
            .unwrap();
        let empty = service(br#"{"players_online":0,"names":[]}"#)
            .fetch_status()
            .await
            .unwrap();
        assert_eq!(absent.names, None);
        assert_eq!(empty.names, Some(vec![]));
    }
}
