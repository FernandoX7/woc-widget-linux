//! Domain logic for WoC Player Count, ported from the macOS `WoCKit` Swift package.
//!
//! This crate is deliberately free of UI and Tauri dependencies: everything here is
//! testable headless. Modules are filled in phase by phase (see docs/linux-port/).

pub mod config;
pub mod crypto_format;
pub mod error;
pub mod formatting;
pub mod http;
pub mod models;
pub mod strings;

/// HTTP clients for the status/community, DEX Screener, and GeckoTerminal feeds (Phase 3).
pub mod services;

/// 7-day player-count history store with coalesced writes (Phase 4).
pub mod history;

/// Trend analytics (Phase 5).
pub mod analytics;

/// Baseline-safe alert engine: hysteresis, cooldown, quiet hours (Phase 6).
pub mod alerts;
