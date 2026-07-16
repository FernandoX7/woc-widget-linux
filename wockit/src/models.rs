//! API response models and domain types ported from the macOS `WoCKit` Swift package.
//!
//! Wire decoding mirrors the hardened Swift `init(from:)` implementations verbatim:
//! required fields hard-fail the decode, ancillary fields are lenient (Swift `try?`
//! semantics: absent, `null`, or wrong-typed becomes `None`), and lossy collections keep
//! their valid rows while refusing to present a fully undecodable feed as empty.
//!
//! Every public type is re-exported here so consumers never spell a submodule.

mod candle;
mod chart_time;
mod community;
mod crypto;
mod decode;
mod gecko;
mod menu_bar;
mod status;

pub use candle::Candle;
pub use chart_time::{CandleInterval, ChartInterval, ChartRange};
pub use community::{
    GameRealm, GameRelease, LifetimeLeaderboard, LifetimeLeaderboardEntry, ProjectStats,
    RealmDirectory, ReleaseFeed,
};
pub use crypto::{
    CryptoMarketTimeframe, CryptoMarketWindow, CryptoQuote, DexLiquidity, DexPair,
    DexRollingValues, DexScreenerResponse, DexToken, DexTransactionCount,
};
pub use gecko::{
    candles_from, ohlcv_endpoint, GeckoOhlcvAttributes, GeckoOhlcvData, GeckoOhlcvResponse,
};
pub use menu_bar::MenuBarDisplayMode;
pub use status::{Sample, StatusResponse};
