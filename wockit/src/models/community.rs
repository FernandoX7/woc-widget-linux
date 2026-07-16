//! Community feed wire models ported from Model/CommunityModels.swift.
//!
//! The feed-state machinery (CommunityFeedState / CommunityFeedError /
//! CommunityFeedPhase) is store-layer and lands with the community store phase.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::de::Error as _;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use url::Url;

use super::decode;

/// Aggregate, privacy-safe project statistics exposed by the first-party API.
/// Swift parity: ProjectStats in Model/CommunityModels.swift.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectStats {
    #[serde(rename = "accounts_created", skip_serializing_if = "Option::is_none")]
    pub accounts_created: Option<i64>,
    #[serde(rename = "players_online", skip_serializing_if = "Option::is_none")]
    pub players_online: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realm: Option<String>,
}

impl<'de> Deserialize<'de> for ProjectStats {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;
        let accounts_created = decode::nonnegative(map.get("accounts_created"));
        let players_online = decode::nonnegative(map.get("players_online"));
        let realm = decode::nonempty_text(map.get("realm"));
        if accounts_created.is_none() && players_online.is_none() && realm.is_none() {
            return Err(D::Error::custom(
                "Project stats contains no usable typed fields",
            ));
        }
        Ok(Self {
            accounts_created,
            players_online,
            realm,
        })
    }
}

/// Swift parity: ReleaseFeed in Model/CommunityModels.swift (wire key `repo`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleaseFeed {
    #[serde(rename = "repo", skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    pub releases: Vec<GameRelease>,
}

impl<'de> Deserialize<'de> for ReleaseFeed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;
        Ok(Self {
            repository: decode::nonempty_text(map.get("repo")),
            releases: decode::lossy_array(map, "releases")?,
        })
    }
}

/// One GitHub-style release row. Decode requires at least one usable identity field
/// (id, tag, name, or url). Round-trips through Serialize for the release-alert
/// snapshots of a later phase.
/// Swift parity: GameRelease in Model/CommunityModels.swift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameRelease {
    pub id: Option<i64>,
    pub tag: Option<String>,
    pub name: Option<String>,
    pub body: Option<String>,
    pub url: Option<Url>,
    /// Wire key `prerelease`; lenient with a `false` default.
    pub is_prerelease: bool,
    /// Wire key `publishedAt`, ISO-8601.
    pub published_at: Option<DateTime<Utc>>,
}

impl<'de> Deserialize<'de> for GameRelease {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;
        let id = decode::nonnegative(map.get("id"));
        let tag = decode::nonempty_text(map.get("tag"));
        let name = decode::nonempty_text(map.get("name"));
        let url = decode::https_url(map.get("url"));
        if id.is_none() && tag.is_none() && name.is_none() && url.is_none() {
            return Err(D::Error::custom("Release contains no usable identity"));
        }
        Ok(Self {
            id,
            tag,
            name,
            body: decode::lenient(map.get("body")),
            url,
            is_prerelease: decode::lenient(map.get("prerelease")).unwrap_or(false),
            published_at: decode::iso_date(map.get("publishedAt")),
        })
    }
}

// Symmetric to the Swift encode(to:): optionals are omitted when absent, the URL is
// encoded as its absolute string, and the date as whole-second ISO-8601.
impl Serialize for GameRelease {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        if let Some(id) = self.id {
            map.serialize_entry("id", &id)?;
        }
        if let Some(tag) = &self.tag {
            map.serialize_entry("tag", tag)?;
        }
        if let Some(name) = &self.name {
            map.serialize_entry("name", name)?;
        }
        if let Some(body) = &self.body {
            map.serialize_entry("body", body)?;
        }
        if let Some(url) = &self.url {
            map.serialize_entry("url", url.as_str())?;
        }
        map.serialize_entry("prerelease", &self.is_prerelease)?;
        if let Some(published_at) = &self.published_at {
            map.serialize_entry("publishedAt", &decode::iso_string(published_at))?;
        }
        map.end()
    }
}

/// Swift parity: LifetimeLeaderboard in Model/CommunityModels.swift.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LifetimeLeaderboard {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    pub leaders: Vec<LifetimeLeaderboardEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    #[serde(rename = "pageCount", skip_serializing_if = "Option::is_none")]
    pub page_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
    #[serde(rename = "pageSize", skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i64>,
}

impl<'de> Deserialize<'de> for LifetimeLeaderboard {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;
        Ok(Self {
            realm: decode::nonempty_text(map.get("realm")),
            scope: decode::nonempty_text(map.get("scope")),
            metric: decode::nonempty_text(map.get("metric")),
            leaders: decode::lossy_array(map, "leaders")?,
            page: decode::nonnegative(map.get("page")),
            page_count: decode::nonnegative(map.get("pageCount")),
            total: decode::nonnegative(map.get("total")),
            page_size: decode::nonnegative(map.get("pageSize")),
        })
    }
}

/// One leaderboard row; the player name is required non-empty, everything else lenient.
/// Swift parity: LifetimeLeaderboardEntry in Model/CommunityModels.swift
/// (wire keys `cls` and `lifetimeXp`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LifetimeLeaderboardEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "cls", skip_serializing_if = "Option::is_none")]
    pub character_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<i64>,
    #[serde(rename = "virtualLevel", skip_serializing_if = "Option::is_none")]
    pub virtual_level: Option<i64>,
    #[serde(rename = "lifetimeXp", skip_serializing_if = "Option::is_none")]
    pub lifetime_xp: Option<i64>,
    #[serde(rename = "prestigeRank", skip_serializing_if = "Option::is_none")]
    pub prestige_rank: Option<i64>,
}

impl<'de> Deserialize<'de> for LifetimeLeaderboardEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;
        let Some(name) = decode::nonempty_text(map.get("name")) else {
            return Err(D::Error::custom(
                "Leaderboard row has no usable player name",
            ));
        };
        Ok(Self {
            rank: decode::nonnegative(map.get("rank")),
            name: Some(name),
            character_class: decode::nonempty_text(map.get("cls")),
            level: decode::nonnegative(map.get("level")),
            virtual_level: decode::nonnegative(map.get("virtualLevel")),
            lifetime_xp: decode::nonnegative(map.get("lifetimeXp")),
            prestige_rank: decode::nonnegative(map.get("prestigeRank")),
        })
    }
}

/// Swift parity: RealmDirectory in Model/CommunityModels.swift (wire keys `current` and
/// `characters`; the character-count map is hard-required, then filtered to non-empty
/// keys with values >= 0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RealmDirectory {
    #[serde(rename = "current", skip_serializing_if = "Option::is_none")]
    pub current_realm: Option<String>,
    pub realms: Vec<GameRealm>,
    #[serde(rename = "characters")]
    pub character_counts_by_realm: HashMap<String, i64>,
}

impl<'de> Deserialize<'de> for RealmDirectory {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;
        let characters = map
            .get("characters")
            .ok_or_else(|| D::Error::custom("characters map is required"))?;
        let raw_counts: HashMap<String, i64> = serde_json::from_value(characters.clone())
            .map_err(|_| D::Error::custom("characters must map realm names to integers"))?;
        Ok(Self {
            current_realm: decode::nonempty_text(map.get("current")),
            realms: decode::lossy_array(map, "realms")?,
            character_counts_by_realm: raw_counts
                .into_iter()
                .filter(|(key, count)| !key.is_empty() && *count >= 0)
                .collect(),
        })
    }
}

/// One realm row; the name is required non-empty, the server URL must be https.
/// Swift parity: GameRealm in Model/CommunityModels.swift (wire keys `url` and `type`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameRealm {
    pub name: Option<String>,
    /// Wire key `url`.
    pub server_url: Option<Url>,
    /// Wire key `type`.
    pub realm_type: Option<String>,
}

impl<'de> Deserialize<'de> for GameRealm {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = decode::as_object::<D::Error>(&value)?;
        let Some(name) = decode::nonempty_text(map.get("name")) else {
            return Err(D::Error::custom("Realm row has no usable name"));
        };
        Ok(Self {
            name: Some(name),
            server_url: decode::https_url(map.get("url")),
            realm_type: decode::nonempty_text(map.get("type")),
        })
    }
}

// Symmetric to the Swift encode(to:): optionals omitted when absent, URL as its
// absolute string.
impl Serialize for GameRealm {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        if let Some(name) = &self.name {
            map.serialize_entry("name", name)?;
        }
        if let Some(url) = &self.server_url {
            map.serialize_entry("url", url.as_str())?;
        }
        if let Some(realm_type) = &self.realm_type {
            map.serialize_entry("type", realm_type)?;
        }
        map.end()
    }
}

// The Swift CommunityServiceTests cases below go through an injected HTTP client; the
// decode rules are identical at the model layer, so each parity test decodes the same
// fixture JSON directly. Transport plumbing (paths, limit clamping, HTTP failure
// mapping) is Phase 3.
#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, serde_json::Error> {
        serde_json::from_str(json)
    }

    fn epoch(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    // Swift parity: CommunityServiceTests.swift > CommunityServiceTests >
    // decodesProjectStatsThroughInjectedHTTPClient (decode assertions only)
    #[test]
    fn decodes_project_stats() {
        let stats: ProjectStats =
            decode(r#"{"accounts_created":48156,"players_online":93,"realm":"Claudemoon"}"#)
                .unwrap();
        assert_eq!(stats.accounts_created, Some(48_156));
        assert_eq!(stats.players_online, Some(93));
        assert_eq!(stats.realm.as_deref(), Some("Claudemoon"));
    }

    // Swift parity: CommunityServiceTests.swift > CommunityServiceTests >
    // projectStatsTreatsChangingAncillaryTypesAsUnavailable
    #[test]
    fn project_stats_treats_changing_ancillary_types_as_unavailable() {
        let stats: ProjectStats = decode(
            r#"{"accounts_created":"many","players_online":93,"realm":false,"new_field":123}"#,
        )
        .unwrap();
        assert_eq!(stats.accounts_created, None);
        assert_eq!(stats.players_online, Some(93));
        assert_eq!(stats.realm, None);
    }

    // Swift parity: CommunityServiceTests.swift > CommunityServiceTests >
    // projectStatsRequiresAtLeastOneUsableTypedField (all three sub-shapes)
    #[test]
    fn project_stats_requires_at_least_one_usable_typed_field() {
        for json in [
            r#"{}"#,
            r#"{"accounts_created":"many","players_online":false,"realm":"   "}"#,
            r#"{"accounts_created":-1,"players_online":-2}"#,
        ] {
            assert!(decode::<ProjectStats>(json).is_err(), "should fail: {json}");
        }
    }

    // Swift parity: CommunityServiceTests.swift > CommunityServiceTests >
    // decodesReleaseFeedDatesLinksAndSkipsMalformedRows
    #[test]
    fn decodes_release_feed_dates_links_and_skips_malformed_rows() {
        let json = r#"
        {
          "repo": "levy-street/world-of-claudecraft",
          "releases": [
            {
              "id": 351405040,
              "tag": "v0.23.0",
              "name": "World of ClaudeCraft v0.23.0",
              "body": "Highlights",
              "url": "https://github.com/levy-street/world-of-claudecraft/releases/tag/v0.23.0",
              "prerelease": false,
              "publishedAt": "2026-07-09T08:25:30Z"
            },
            42,
            {"id":"bad","tag":"v-next","url":"relative/path","publishedAt":"not-a-date"}
          ]
        }"#;

        let feed: ReleaseFeed = decode(json).unwrap();
        assert_eq!(
            feed.repository.as_deref(),
            Some("levy-street/world-of-claudecraft")
        );
        assert_eq!(feed.releases.len(), 2);
        assert_eq!(feed.releases[0].id, Some(351_405_040));
        assert_eq!(feed.releases[0].tag.as_deref(), Some("v0.23.0"));
        assert_eq!(feed.releases[0].published_at, Some(epoch(1_783_585_530)));
        assert_eq!(
            feed.releases[0].url.as_ref().and_then(Url::host_str),
            Some("github.com")
        );
        assert_eq!(feed.releases[1].tag.as_deref(), Some("v-next"));
        assert_eq!(feed.releases[1].id, None);
        assert_eq!(feed.releases[1].url, None);
        assert_eq!(feed.releases[1].published_at, None);
    }

    // Swift parity: CommunityServiceTests.swift > CommunityServiceTests >
    // acceptsFractionalISOReleaseTimestamp
    #[test]
    fn accepts_fractional_iso_release_timestamp() {
        let feed: ReleaseFeed =
            decode(r#"{"releases":[{"tag":"v-next","publishedAt":"2026-07-09T08:25:30.123Z"}]}"#)
                .unwrap();
        assert!(feed.releases[0].published_at.is_some());
    }

    // Swift parity: CommunityServiceTests.swift > CommunityServiceTests >
    // decodesLifetimeLeaderboardAndDefaultsBadOptionalFields
    #[test]
    fn decodes_lifetime_leaderboard_and_defaults_bad_optional_fields() {
        let json = r#"
        {
          "realm":"Claudemoon", "scope":"realm", "metric":"lifetimeXp",
          "leaders":[
            {"rank":1,"name":"Moonwarden","cls":"hunter","level":20,"virtualLevel":38,
             "lifetimeXp":1236074,"prestigeRank":46},
            {"rank":"second","name":"Emberguard","cls":7,"level":20}
          ],
          "page":0,"pageCount":1,"total":2,"pageSize":2
        }"#;

        let board: LifetimeLeaderboard = decode(json).unwrap();
        assert_eq!(board.realm.as_deref(), Some("Claudemoon"));
        assert_eq!(board.leaders.len(), 2);
        assert_eq!(board.leaders[0].character_class.as_deref(), Some("hunter"));
        assert_eq!(board.leaders[0].lifetime_xp, Some(1_236_074));
        assert_eq!(board.leaders[1].name.as_deref(), Some("Emberguard"));
        assert_eq!(board.leaders[1].rank, None);
        assert_eq!(board.leaders[1].character_class, None);
        assert_eq!(board.total, Some(2));
    }

    // Swift parity: CommunityServiceTests.swift > CommunityServiceTests >
    // requiredCollectionKeysMustBePresentArrays (all 8 sub-cases, including the
    // characters-as-array failure)
    #[test]
    fn required_collection_keys_must_be_present_arrays() {
        assert!(decode::<ReleaseFeed>(r#"{"repo":"owner/repo"}"#).is_err());
        assert!(decode::<ReleaseFeed>(r#"{"releases":{}}"#).is_err());
        assert!(decode::<LifetimeLeaderboard>(r#"{"realm":"Claudemoon"}"#).is_err());
        assert!(decode::<LifetimeLeaderboard>(r#"{"leaders":{}}"#).is_err());
        assert!(decode::<RealmDirectory>(r#"{"current":"Claudemoon"}"#).is_err());
        assert!(decode::<RealmDirectory>(r#"{"realms":{}}"#).is_err());
        assert!(decode::<RealmDirectory>(r#"{"realms":[]}"#).is_err());
        assert!(decode::<RealmDirectory>(r#"{"realms":[],"characters":[]}"#).is_err());
    }

    // Swift parity: CommunityServiceTests.swift > CommunityServiceTests >
    // nonemptyCollectionsWithNoDecodableRowsFailInsteadOfLookingEmpty
    #[test]
    fn nonempty_collections_with_no_decodable_rows_fail_instead_of_looking_empty() {
        assert!(decode::<ReleaseFeed>(r#"{"releases":[42,false,{}]}"#).is_err());
        assert!(decode::<LifetimeLeaderboard>(r#"{"leaders":[42,false,{}]}"#).is_err());
        assert!(decode::<RealmDirectory>(r#"{"realms":[42,false,{}],"characters":{}}"#).is_err());
    }

    // Swift parity: CommunityServiceTests.swift > CommunityServiceTests >
    // mixedCollectionsKeepTheirDecodableRows
    #[test]
    fn mixed_collections_keep_their_decodable_rows() {
        let leaderboard: LifetimeLeaderboard =
            decode(r#"{"leaders":[42,{"rank":1,"name":"Valid"}]}"#).unwrap();
        let realms: RealmDirectory =
            decode(r#"{"realms":[false,{"name":"Claudemoon"}],"characters":{}}"#).unwrap();

        assert_eq!(
            leaderboard
                .leaders
                .iter()
                .map(|entry| entry.name.as_deref())
                .collect::<Vec<_>>(),
            [Some("Valid")]
        );
        assert_eq!(
            realms
                .realms
                .iter()
                .map(|realm| realm.name.as_deref())
                .collect::<Vec<_>>(),
            [Some("Claudemoon")]
        );
    }

    // Swift parity: CommunityServiceTests.swift > CommunityServiceTests >
    // emptyRequiredCollectionsRemainValidSuccessfulResponses
    #[test]
    fn empty_required_collections_remain_valid_successful_responses() {
        let releases: ReleaseFeed = decode(r#"{"releases":[]}"#).unwrap();
        let leaderboard: LifetimeLeaderboard = decode(r#"{"leaders":[]}"#).unwrap();
        let realms: RealmDirectory = decode(r#"{"realms":[],"characters":{}}"#).unwrap();

        assert!(releases.releases.is_empty());
        assert!(leaderboard.leaders.is_empty());
        assert!(realms.realms.is_empty());
    }

    // Swift parity: CommunityServiceTests.swift > CommunityServiceTests >
    // decodesAnonymousRealmDirectory
    #[test]
    fn decodes_anonymous_realm_directory() {
        let json = r#"
        {"current":"Claudemoon","realms":[
          {"name":"Claudemoon","url":"","type":"Normal"},
          {"name":"Test","url":"https://test.example/game","type":"Seasonal"}
        ],"characters":{"Claudemoon":2}}"#;

        let directory: RealmDirectory = decode(json).unwrap();
        assert_eq!(directory.current_realm.as_deref(), Some("Claudemoon"));
        assert_eq!(directory.realms.len(), 2);
        assert_eq!(directory.realms[0].server_url, None);
        assert_eq!(
            directory.realms[1].server_url.as_ref().map(Url::as_str),
            Some("https://test.example/game")
        );
        assert_eq!(
            directory.character_counts_by_realm.get("Claudemoon"),
            Some(&2)
        );
    }

    // Swift parity: CommunityServiceTests.swift > CommunityServiceTests >
    // communityLinksRequireHTTPS
    #[test]
    fn community_links_require_https() {
        let feed: ReleaseFeed =
            decode(r#"{"releases":[{"tag":"v1","url":"http://example.com/release"}]}"#).unwrap();
        assert_eq!(feed.releases[0].url, None);
    }

    // No direct Swift test case; guards the symmetric encode(to:) implementations that
    // the release-alert snapshots of a later phase depend on (GameRelease and GameRealm
    // in Model/CommunityModels.swift).
    #[test]
    fn game_release_and_game_realm_round_trip_through_serialize() {
        let json = r#"
        {"releases":[{
          "id": 7,
          "tag": "v1.2.3",
          "name": "Release",
          "body": "Notes",
          "url": "https://github.com/owner/repo/releases/tag/v1.2.3",
          "prerelease": true,
          "publishedAt": "2026-07-09T08:25:30Z"
        }]}"#;
        let feed: ReleaseFeed = decode(json).unwrap();
        let release = &feed.releases[0];
        let reencoded = serde_json::to_string(release).unwrap();
        let round_tripped: GameRelease = decode(&reencoded).unwrap();
        assert_eq!(&round_tripped, release);
        assert!(reencoded.contains(r#""publishedAt":"2026-07-09T08:25:30Z""#));

        let realm: GameRealm =
            decode(r#"{"name":"Test","url":"https://test.example/game","type":"Seasonal"}"#)
                .unwrap();
        let reencoded = serde_json::to_string(&realm).unwrap();
        let round_tripped: GameRealm = decode(&reencoded).unwrap();
        assert_eq!(round_tripped, realm);
    }
}
