//! First-party community feed client.

use crate::error::FetchError;
use crate::http::{fetch_decoded, HttpTransport, ReqwestTransport};
use crate::models::{LifetimeLeaderboard, ProjectStats, RealmDirectory, ReleaseFeed};

const PRODUCTION_BASE_URL: &str = "https://worldofclaudecraft.com";

/// One-shot reads from World of ClaudeCraft's public API. Each method is independent;
/// the service deliberately owns no shared feed state, timer, or retry policy.
#[derive(Clone, Debug)]
pub struct CommunityService<T = ReqwestTransport> {
    transport: T,
    base_url: url::Url,
}

impl Default for CommunityService<ReqwestTransport> {
    fn default() -> Self {
        Self::new()
    }
}

impl CommunityService<ReqwestTransport> {
    /// Live service for the first-party API.
    pub fn new() -> Self {
        Self::with_transport(
            ReqwestTransport::new(),
            url::Url::parse(PRODUCTION_BASE_URL).expect("production community URL is valid"),
        )
    }
}

impl<T: HttpTransport> CommunityService<T> {
    /// Injectable construction seam used by parity tests.
    pub fn with_transport(transport: T, base_url: url::Url) -> Self {
        Self {
            transport,
            base_url,
        }
    }

    pub async fn fetch_project_stats(&self) -> Result<ProjectStats, FetchError> {
        fetch_decoded(&self.transport, &self.endpoint("api/project-stats", None)).await
    }

    pub async fn fetch_releases(&self, limit: i64) -> Result<ReleaseFeed, FetchError> {
        fetch_decoded(
            &self.transport,
            &self.endpoint("api/releases", Some(("limit", limit.max(1).to_string()))),
        )
        .await
    }

    pub async fn fetch_leaderboard(&self, limit: i64) -> Result<LifetimeLeaderboard, FetchError> {
        fetch_decoded(
            &self.transport,
            &self.endpoint("api/leaderboard", Some(("limit", limit.max(1).to_string()))),
        )
        .await
    }

    pub async fn fetch_realms(&self) -> Result<RealmDirectory, FetchError> {
        fetch_decoded(&self.transport, &self.endpoint("api/realms", None)).await
    }

    /// Build a first-party endpoint beneath the configured base path.
    pub fn endpoint(&self, path: &str, query: Option<(&str, String)>) -> url::Url {
        let mut url = self.base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .expect("community base URL must support hierarchical paths");
            segments.pop_if_empty();
            segments.extend(path.split('/').filter(|component| !component.is_empty()));
        }
        if let Some((name, value)) = query {
            url.query_pairs_mut().append_pair(name, &value);
        }
        url
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;
    use crate::error::TransportErrorKind;
    use crate::http::HttpResponse;

    struct RoutingTransport {
        responses: HashMap<&'static str, (u16, &'static str)>,
    }

    impl HttpTransport for RoutingTransport {
        async fn get(&self, url: &url::Url) -> Result<HttpResponse, TransportErrorKind> {
            let (status, body) = self.responses.get(url.path()).copied().unwrap();
            Ok(HttpResponse {
                status,
                body: body.as_bytes().to_vec(),
            })
        }
    }

    fn service(
        responses: &[(&'static str, u16, &'static str)],
    ) -> CommunityService<RoutingTransport> {
        CommunityService::with_transport(
            RoutingTransport {
                responses: responses
                    .iter()
                    .map(|(path, status, body)| (*path, (*status, *body)))
                    .collect(),
            },
            url::Url::parse("https://example.invalid/root").unwrap(),
        )
    }

    struct CapturingTransport {
        urls: Mutex<Vec<String>>,
    }

    impl HttpTransport for CapturingTransport {
        async fn get(&self, url: &url::Url) -> Result<HttpResponse, TransportErrorKind> {
            self.urls.lock().unwrap().push(url.to_string());
            let body = if url.path().ends_with("/releases") {
                r#"{"releases":[]}"#
            } else {
                r#"{"leaders":[]}"#
            };
            Ok(HttpResponse {
                status: 200,
                body: body.as_bytes().to_vec(),
            })
        }
    }

    #[tokio::test]
    async fn decodes_all_four_feeds_through_shared_transport() {
        let service = service(&[
            (
                "/root/api/project-stats",
                200,
                r#"{"accounts_created":48156,"players_online":93,"realm":"Claudemoon"}"#,
            ),
            (
                "/root/api/releases",
                200,
                r#"{"releases":[42,{"tag":"v-next"}]}"#,
            ),
            (
                "/root/api/leaderboard",
                200,
                r#"{"leaders":[false,{"rank":1,"name":"Moonwarden"}]}"#,
            ),
            (
                "/root/api/realms",
                200,
                r#"{"current":"Claudemoon","realms":[0,{"name":"Claudemoon"}],"characters":{"Claudemoon":2}}"#,
            ),
        ]);

        assert_eq!(
            service.fetch_project_stats().await.unwrap().players_online,
            Some(93)
        );
        assert_eq!(service.fetch_releases(3).await.unwrap().releases.len(), 1);
        assert_eq!(service.fetch_leaderboard(5).await.unwrap().leaders.len(), 1);
        assert_eq!(service.fetch_realms().await.unwrap().realms.len(), 1);
    }

    #[tokio::test]
    async fn one_feed_failure_does_not_affect_the_other_feeds() {
        let service = service(&[
            ("/root/api/project-stats", 200, r#"{"players_online":93}"#),
            ("/root/api/releases", 503, "{}"),
            (
                "/root/api/leaderboard",
                200,
                r#"{"leaders":[{"name":"Moonwarden"}]}"#,
            ),
            ("/root/api/realms", 200, r#"{"realms":[],"characters":{}}"#),
        ]);

        assert_eq!(service.fetch_releases(3).await, Err(FetchError::Http(503)));
        assert_eq!(
            service.fetch_project_stats().await.unwrap().players_online,
            Some(93)
        );
        assert_eq!(service.fetch_leaderboard(5).await.unwrap().leaders.len(), 1);
        assert!(service.fetch_realms().await.unwrap().realms.is_empty());
    }

    #[test]
    fn constructs_first_party_paths() {
        let service = service(&[]);
        assert_eq!(
            service.endpoint("api/project-stats", None).as_str(),
            "https://example.invalid/root/api/project-stats"
        );
        assert_eq!(
            service
                .endpoint("api/releases", Some(("limit", "1".to_string())))
                .as_str(),
            "https://example.invalid/root/api/releases?limit=1"
        );
    }

    #[tokio::test]
    async fn clamps_nonpositive_limits_before_sending_requests() {
        let service = CommunityService::with_transport(
            CapturingTransport {
                urls: Mutex::new(Vec::new()),
            },
            url::Url::parse("https://example.invalid").unwrap(),
        );

        service.fetch_releases(0).await.unwrap();
        service.fetch_leaderboard(-5).await.unwrap();

        assert_eq!(
            *service.transport.urls.lock().unwrap(),
            [
                "https://example.invalid/api/releases?limit=1",
                "https://example.invalid/api/leaderboard?limit=1",
            ]
        );
    }
}
