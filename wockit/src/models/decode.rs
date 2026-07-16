//! Shared decoding helpers mirroring the Swift `try?` field leniency and the
//! `CommunityModelCoding` value sanitizers (CommunityModels.swift).

use chrono::{DateTime, SecondsFormat, Utc};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use url::Url;

/// Swift `try? container.decode(T.self, forKey:)` twin: an absent key, JSON `null`, or a
/// wrong-typed value becomes `None` instead of failing the enclosing decode.
pub(super) fn lenient<T: DeserializeOwned>(value: Option<&Value>) -> Option<T> {
    value.and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Requires the decoded document to be a JSON object, like Swift's
/// `decoder.container(keyedBy:)`.
pub(super) fn as_object<E: serde::de::Error>(value: &Value) -> Result<&Map<String, Value>, E> {
    value
        .as_object()
        .ok_or_else(|| E::custom("expected a JSON object"))
}

/// Swift parity: CommunityModelCoding.nonemptyText - lenient string, trimmed; empty or
/// wrong-typed becomes `None`.
pub(super) fn nonempty_text(value: Option<&Value>) -> Option<String> {
    let text: String = lenient(value)?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Swift parity: CommunityModelCoding.nonnegative - lenient integer, `None` unless >= 0.
pub(super) fn nonnegative(value: Option<&Value>) -> Option<i64> {
    lenient::<i64>(value).filter(|v| *v >= 0)
}

/// Swift parity: CommunityModelCoding.httpsURL(from:) - only absolute `https` URLs with a
/// host survive; `http://`, relative paths, and empty strings all become `None`.
/// (`Url::parse` lowercases the scheme, matching Swift's `scheme?.lowercased()` check.)
pub(super) fn https_url(value: Option<&Value>) -> Option<Url> {
    let raw: String = lenient(value)?;
    if raw.is_empty() {
        return None;
    }
    let url = Url::parse(&raw).ok()?;
    if url.scheme() == "https" && url.host_str().is_some() {
        Some(url)
    } else {
        None
    }
}

/// Swift parity: CommunityModelCoding.date(from:) - ISO-8601 with or without fractional
/// seconds ("2026-07-09T08:25:30Z" and "...30.123Z"). chrono's RFC 3339 parser accepts a
/// slight superset (e.g. numeric UTC offsets); benign drift for a lenient field.
pub(super) fn iso_date(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let raw: String = lenient(value)?;
    DateTime::parse_from_rfc3339(&raw)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

/// Swift parity: CommunityModelCoding.string(from:) - the whole-second UTC "Z" form
/// `ISO8601DateFormatter` emits.
pub(super) fn iso_string(date: &DateTime<Utc>) -> String {
    date.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Swift parity: KeyedDecodingContainer.decodeLossyArray - the key must be present and an
/// array (hard error otherwise); per-element failures drop the row; a nonempty array with
/// zero decodable rows is a hard error rather than a plausible-looking empty feed.
pub(super) fn lossy_array<T: DeserializeOwned, E: serde::de::Error>(
    map: &Map<String, Value>,
    key: &str,
) -> Result<Vec<T>, E> {
    let raw = map
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| E::custom(format!("{key} must be a present array")))?;
    let survivors: Vec<T> = raw
        .iter()
        .filter_map(|element| serde_json::from_value(element.clone()).ok())
        .collect();
    if !raw.is_empty() && survivors.is_empty() {
        return Err(E::custom("Nonempty collection contains no decodable rows"));
    }
    Ok(survivors)
}
