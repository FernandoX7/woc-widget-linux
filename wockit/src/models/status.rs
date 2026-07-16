//! First-party realm status wire model and the player-count history sample.

use chrono::{DateTime, Utc};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::decode;

/// Wire response of the first-party status endpoint.
/// Swift parity: StatusResponse in Model/Models.swift.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realm: Option<String>,
    #[serde(rename = "players_online")]
    pub players_online: i64,
    /// `None` means this API deployment does not expose roster data. An explicit empty
    /// vec means the capability exists and no names were returned. Keeping those states
    /// distinct prevents a privacy-disabled roster from being presented as "0 online"
    /// while the aggregate count is > 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<String>>,
}

impl StatusResponse {
    /// Direct (memberwise-style) constructor, used by preview seeding and tests.
    /// Swift clamps a negative count to zero here.
    pub fn new(
        ok: bool,
        realm: Option<String>,
        players_online: i64,
        names: Option<Vec<String>>,
    ) -> Self {
        Self {
            ok,
            realm,
            players_online: players_online.max(0),
            names,
        }
    }

    pub fn has_roster_capability(&self) -> bool {
        self.names.is_some()
    }
}

// Hardened decoding ported from the Swift extension: only the core `players_online`
// count is required - a parseable 200 is always "up". `names` stays `None` when absent
// or wrong-typed and `ok` defaults to `true` when absent. A present but wrong-typed `ok`
// is a schema failure rather than evidence of health (a lying schema must not read as
// healthy); the store classifies that as an invalid response and never confirms an
// outage from it.
impl<'de> Deserialize<'de> for StatusResponse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;

        let players_online = map
            .get("players_online")
            .and_then(Value::as_i64)
            .ok_or_else(|| D::Error::custom("players_online must be an integer"))?;
        if players_online < 0 {
            return Err(D::Error::custom("players_online cannot be negative"));
        }

        // Swift `try?`: present-but-wrong-typed or absent roster -> None.
        let names = decode::lenient(map.get("names"));

        // Swift decodeIfPresent semantics: absent or null defaults true, wrong type throws.
        let ok = match map.get("ok") {
            None | Some(Value::Null) => true,
            Some(Value::Bool(flag)) => *flag,
            Some(_) => return Err(D::Error::custom("ok must be a boolean when present")),
        };

        // Swift decodeIfPresent(String) throws on a type mismatch; a present string is
        // trimmed and an empty result becomes None.
        let realm = match map.get("realm") {
            None | Some(Value::Null) => None,
            Some(Value::String(raw)) => {
                let trimmed = raw.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }
            Some(_) => return Err(D::Error::custom("realm must be a string when present")),
        };

        Ok(Self {
            ok,
            realm,
            players_online,
            names,
        })
    }
}

/// One player-count history point. Swift parity: Sample in Model/Models.swift.
///
/// The frozen on-disk history format (JSON array of `{date: ISO-8601 string, count: Int}`
/// at `$XDG_DATA_HOME/woc-widget/history.json`) is owned by the Phase 4 history store;
/// this type only mirrors the in-memory Codable twin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sample {
    pub date: DateTime<Utc>,
    pub count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(json: &str) -> Result<StatusResponse, serde_json::Error> {
        serde_json::from_str(json)
    }

    // Swift parity: DecodingErrorTests.swift > StatusResponseDecodingTests >
    // distinguishesUnavailableRosterAndDefaultsOkTrueWhenAbsent
    #[test]
    fn distinguishes_unavailable_roster_and_defaults_ok_true_when_absent() {
        let r = decode(r#"{"players_online": 42}"#).unwrap();
        assert_eq!(r.players_online, 42);
        assert_eq!(r.names, None); // absent means the endpoint does not expose this capability
        assert!(!r.has_roster_capability());
        assert!(r.ok); // hardened: missing ok -> true (no false "down" on a healthy 200)
        assert_eq!(r.realm, None);
    }

    // Swift parity: DecodingErrorTests.swift > StatusResponseDecodingTests > decodesFullResponse
    #[test]
    fn decodes_full_response() {
        let r = decode(
            r#"{"ok": false, "realm": "Claudemoon", "players_online": 3, "names": ["a","b"]}"#,
        )
        .unwrap();
        assert!(!r.ok);
        assert_eq!(r.realm.as_deref(), Some("Claudemoon"));
        assert_eq!(r.players_online, 3);
        assert_eq!(r.names, Some(vec!["a".to_string(), "b".to_string()]));
    }

    // Swift parity: DecodingErrorTests.swift > StatusResponseDecodingTests >
    // throwsWhenCoreCountMissing
    #[test]
    fn throws_when_core_count_missing() {
        assert!(decode(r#"{"ok": true, "names": []}"#).is_err());
    }

    // Swift parity: DecodingErrorTests.swift > StatusResponseDecodingTests >
    // optionalRosterTypeCanDriftButPresentStatusTypeMustBeTruthful
    #[test]
    fn optional_roster_type_can_drift_but_present_status_type_must_be_truthful() {
        let r = decode(r#"{"players_online": 4, "names": 5, "realm": "  Claudemoon  "}"#).unwrap();
        assert_eq!(r.players_online, 4);
        assert!(r.ok); // absent ok still defaults true
        assert_eq!(r.names, None); // wrong-typed names -> capability unavailable
        assert_eq!(r.realm.as_deref(), Some("Claudemoon"));
        assert!(decode(r#"{"players_online": 4, "ok": "yes"}"#).is_err());
    }

    // Swift parity: DecodingErrorTests.swift > StatusResponseDecodingTests >
    // throwsWhenCoreCountWrongType
    #[test]
    fn throws_when_core_count_wrong_type() {
        assert!(decode(r#"{"players_online": "lots"}"#).is_err()); // core count must be an integer
    }

    // Swift parity: DecodingErrorTests.swift > StatusResponseDecodingTests >
    // throwsWhenCoreCountIsNegative
    #[test]
    fn throws_when_core_count_is_negative() {
        assert!(decode(r#"{"players_online": -1}"#).is_err());
    }

    // No direct Swift test case; guards the documented None-vs-empty roster distinction
    // and the memberwise clamp in Models.swift StatusResponse (init(ok:realm:playersOnline:names:)).
    #[test]
    fn empty_roster_stays_present_and_memberwise_init_clamps_negative_counts() {
        let r = decode(r#"{"players_online": 0, "names": []}"#).unwrap();
        assert_eq!(r.names, Some(vec![])); // present-but-none, not capability-absent
        assert!(r.has_roster_capability());

        let clamped = StatusResponse::new(true, None, -5, None);
        assert_eq!(clamped.players_online, 0);
    }
}
