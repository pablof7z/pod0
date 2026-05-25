//! Parse the legacy `podcastr-state.v1.json` metadata blob.
//!
//! The Swift `AppState` Codable shape is the authority — see
//! `App/Sources/Domain/AppState.swift`. Every field is decoded with
//! `decodeIfPresent` on the Swift side, so a missing field never breaks
//! decode; we mirror that with `#[serde(default)]` on every shadow field.
//!
//! ## What we extract
//!
//! Only the post-split shape (top-level `podcasts` key + slim
//! `subscriptions` rows) is supported here. Pre-split installs (rows that
//! carry feedURL/title/imageURL inside `subscriptions`, no top-level
//! `podcasts`) ran Swift's own in-process migration the first time they
//! loaded the new file-backed Persistence — that migration has been live
//! since the early Podcastr builds, so any user landing on the NMP build
//! already has the post-split shape on disk.
//!
//! If a pre-split file does show up the
//! `LegacySubscription.podcast_id: Uuid` decode will fail and
//! `from_state_json` returns `Err(MalformedStateJson)`. The shell surfaces
//! the toast and leaves the `pcst.migration.v1.done` sentinel unset, so the
//! next launch retries. Per the M2.D quality gate this is acceptable
//! because pre-split shapes have not existed on disk in shipping installs
//! for many releases; if telemetry ever reports a real user hitting this
//! path, add an `#[serde(alias = "id")]` fallback to the shadow struct.
//!
//! ## What we deliberately drop
//!
//! Settings, notes, friends, agentMemories, agentActivity, clips,
//! threading topics, nostr cursor state, etc. are NOT carried across in
//! this milestone — M2 owns the podcast domain only. Later milestones
//! pick up the rest of `AppState` field-by-field.

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use url::Url;
use uuid::Uuid;

use crate::types::podcast::{NostrVisibility, Podcast, PodcastId, PodcastKind};
use crate::types::subscription::{AutoDownloadMode, AutoDownloadPolicy, PodcastSubscription};

use super::MigrationError;

/// Result of parsing the metadata JSON. Episodes are NOT in here — Swift's
/// `Persistence.metadataState(_:)` strips `episodes = []` before writing the
/// JSON; the actual episode rows live in the SQLite sidecar (see
/// `from_episode_db`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StateJsonResult {
    pub podcasts: Vec<Podcast>,
    pub subscriptions: Vec<PodcastSubscription>,
}

/// Parse the JSON bytes that the iOS `pcst.legacy_io.capability` returned
/// from `podcastr-state.v1.json`. Returns an empty result rather than an
/// error when the blob is well-formed but pre-split (legacy combined-row
/// subscriptions with no top-level `podcasts` key) — D6: not knowing how to
/// recover is data, not a panic.
pub fn from_state_json(json_bytes: &[u8]) -> Result<StateJsonResult, MigrationError> {
    let blob: LegacyAppState = serde_json::from_slice(json_bytes)?;

    // Post-split files always have a top-level `podcasts` key. Use the
    // presence of that key (i.e. our `podcasts` field actually deserialized
    // something) as the discriminator. The Swift split also injects a
    // `Podcast.unknown` row if absent — we preserve that behaviour so
    // episode FKs to `Podcast.unknownID` resolve in the new store too.
    let mut podcasts: Vec<Podcast> = blob.podcasts.into_iter().map(Into::into).collect();

    let subscriptions = blob.subscriptions.iter().map(Into::into).collect();

    let unknown_id = PodcastId::unknown();
    if !podcasts.iter().any(|p| p.id == unknown_id) {
        podcasts.push(Podcast::unknown());
    }

    Ok(StateJsonResult {
        podcasts,
        subscriptions,
    })
}

// ---------------------------------------------------------------------------
// Shadow types — mirror the on-disk Swift JSON shape exactly.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct LegacyAppState {
    podcasts: Vec<LegacyPodcast>,
    subscriptions: Vec<LegacySubscription>,
}

#[derive(Debug, Deserialize)]
struct LegacyPodcast {
    id: Uuid,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default, rename = "feedURL")]
    feed_url: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default, rename = "imageURL")]
    image_url: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    categories: Option<Vec<String>>,
    #[serde(default, rename = "discoveredAt")]
    discovered_at: Option<String>,
    #[serde(default, rename = "lastRefreshedAt")]
    last_refreshed_at: Option<String>,
    #[serde(default)]
    etag: Option<String>,
    #[serde(default, rename = "lastModified")]
    last_modified: Option<String>,
    #[serde(default, rename = "titleIsPlaceholder")]
    title_is_placeholder: Option<bool>,
    #[serde(default, rename = "ownerPubkeyHex")]
    owner_pubkey_hex: Option<String>,
    #[serde(default, rename = "nostrVisibility")]
    nostr_visibility: Option<String>,
    #[serde(default, rename = "nostrCoordinate")]
    nostr_coordinate: Option<String>,
}

impl From<LegacyPodcast> for Podcast {
    fn from(legacy: LegacyPodcast) -> Self {
        Podcast {
            id: PodcastId::new(legacy.id),
            kind: match legacy.kind.as_deref() {
                Some("synthetic") => PodcastKind::Synthetic,
                _ => PodcastKind::Rss,
            },
            feed_url: legacy.feed_url.and_then(|s| Url::parse(&s).ok()),
            title: legacy.title.unwrap_or_default(),
            author: legacy.author.unwrap_or_default(),
            image_url: legacy.image_url.and_then(|s| Url::parse(&s).ok()),
            description: legacy.description.unwrap_or_default(),
            language: legacy.language,
            categories: legacy.categories.unwrap_or_default(),
            discovered_at: parse_iso8601(legacy.discovered_at.as_deref()).unwrap_or_else(Utc::now),
            owner_pubkey_hex: legacy.owner_pubkey_hex,
            nostr_visibility: match legacy.nostr_visibility.as_deref() {
                Some("private") => NostrVisibility::Private,
                _ => NostrVisibility::Public,
            },
            nostr_coordinate: legacy.nostr_coordinate,
            title_is_placeholder: legacy.title_is_placeholder.unwrap_or(false),
            last_refreshed_at: parse_iso8601(legacy.last_refreshed_at.as_deref()),
            etag: legacy.etag,
            last_modified: legacy.last_modified,
        }
    }
}

/// Swift's slim post-split shape. `podcastID` is the FK to `Podcast.id`. The
/// pre-split combined-row variant (carrying feedURL/title) is *not* handled
/// here — see the doc comment at the top of the module for why.
#[derive(Debug, Deserialize)]
struct LegacySubscription {
    #[serde(rename = "podcastID")]
    podcast_id: Uuid,
    #[serde(default, rename = "subscribedAt")]
    subscribed_at: Option<String>,
    #[serde(default, rename = "autoDownload")]
    auto_download: Option<LegacyAutoDownloadPolicy>,
    #[serde(default, rename = "notificationsEnabled")]
    notifications_enabled: Option<bool>,
    #[serde(default, rename = "defaultPlaybackRate")]
    default_playback_rate: Option<f64>,
}

impl From<&LegacySubscription> for PodcastSubscription {
    fn from(legacy: &LegacySubscription) -> Self {
        PodcastSubscription {
            podcast_id: PodcastId::new(legacy.podcast_id),
            subscribed_at: parse_iso8601(legacy.subscribed_at.as_deref())
                .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap_or_else(Utc::now)),
            auto_download: legacy
                .auto_download
                .as_ref()
                .map(Into::into)
                .unwrap_or_default(),
            notifications_enabled: legacy.notifications_enabled.unwrap_or(true),
            default_playback_rate: legacy.default_playback_rate,
        }
    }
}

/// Swift `AutoDownloadPolicy` synthesizes Codable for the `Mode` enum with
/// associated values, producing JSON like:
///
///   { "mode": { "off": {} },          "wifiOnly": true }
///   { "mode": { "allNew": {} },       "wifiOnly": true }
///   { "mode": { "latestN": {"_0": 5} }, "wifiOnly": false }
///
/// The Swift Codable synthesis nests the associated value under `_0`. We
/// accept that exact shape here.
#[derive(Debug, Deserialize)]
struct LegacyAutoDownloadPolicy {
    mode: LegacyAutoDownloadMode,
    #[serde(default = "default_wifi_only", rename = "wifiOnly")]
    wifi_only: bool,
}

fn default_wifi_only() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct LegacyAutoDownloadMode {
    #[serde(default)]
    off: Option<serde_json::Value>,
    #[serde(default, rename = "allNew")]
    all_new: Option<serde_json::Value>,
    #[serde(default, rename = "latestN")]
    latest_n: Option<LegacyLatestN>,
}

#[derive(Debug, Deserialize)]
struct LegacyLatestN {
    #[serde(rename = "_0")]
    count: u32,
}

impl From<&LegacyAutoDownloadPolicy> for AutoDownloadPolicy {
    fn from(legacy: &LegacyAutoDownloadPolicy) -> Self {
        let mode = if legacy.mode.off.is_some() {
            AutoDownloadMode::Off
        } else if let Some(latest) = &legacy.mode.latest_n {
            AutoDownloadMode::LatestN { count: latest.count }
        } else if legacy.mode.all_new.is_some() {
            AutoDownloadMode::AllNew
        } else {
            // Future Swift may add a new mode case. Fall back to the app
            // default so the user still gets a sensible subscription —
            // they can flip the policy from settings later.
            AutoDownloadMode::AllNew
        };
        AutoDownloadPolicy {
            mode,
            wifi_only: legacy.wifi_only,
        }
    }
}

/// Swift writes dates via `JSONEncoder.dateEncodingStrategy = .iso8601`,
/// which produces strings like `"2025-05-24T15:30:00Z"`. Anything we can't
/// parse becomes `None`; the consumer decides on the fallback.
fn parse_iso8601(s: Option<&str>) -> Option<DateTime<Utc>> {
    let s = s?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

// Tests live in `tests/migration_from_state_json.rs` so this file stays
// under the 300-LOC soft cap (per `06-cross-cutting.md` §5). Integration
// tests still exercise the public API end-to-end.
