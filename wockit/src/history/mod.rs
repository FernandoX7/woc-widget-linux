//! Frozen-format player-count history persistence and export.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

pub mod export;
pub mod normalizer;
pub mod peak;
pub mod store;

/// One observation in the frozen on-disk history format.
///
/// The serialized field names are a compatibility contract with the macOS app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistorySample {
    #[serde(rename = "date", with = "whole_second_date")]
    pub date: DateTime<Utc>,
    #[serde(rename = "count")]
    pub count: i64,
}

mod whole_second_date {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&date.to_rfc3339_opts(SecondsFormat::Secs, true))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        DateTime::parse_from_rfc3339(&value)
            .map(|date| date.with_timezone(&Utc))
            .map_err(serde::de::Error::custom)
    }
}

impl HistorySample {
    pub const fn new(date: DateTime<Utc>, count: i64) -> Self {
        Self { date, count }
    }
}
