//! Explicitly invoked API-drift smoke for Phase 3.
//!
//! Kept ignored so the normal suite stays hermetic and offline.

use wockit::models::CandleInterval;
use wockit::services::{CommunityService, CryptoService, GeckoTerminalService, StatusService};

#[tokio::test]
#[ignore = "live API drift smoke; run explicitly"]
async fn configured_endpoints_still_decode() {
    let status = StatusService::live().fetch_status().await.unwrap();
    assert!(status.players_online >= 0);

    let quote = CryptoService::live().fetch_quote().await.unwrap();
    assert!(!quote.price.is_empty());

    let candles = GeckoTerminalService::new()
        .fetch_candles(CandleInterval::OneHour)
        .await
        .unwrap();
    assert!(!candles.is_empty());

    let community = CommunityService::new();
    community.fetch_project_stats().await.unwrap();
    community.fetch_releases(3).await.unwrap();
    community.fetch_leaderboard(5).await.unwrap();
    community.fetch_realms().await.unwrap();
}
