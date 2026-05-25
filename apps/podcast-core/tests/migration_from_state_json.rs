//! Integration tests for `podcast_core::migration::from_state_json`.
//!
//! Lives outside the crate's `src/` tree to keep
//! `src/migration/from_state_json.rs` under the 300-LOC soft cap. The
//! capability-side behaviour (reading the actual `podcastr-state.v1.json`
//! file out of the App Group) is verified separately on iOS — these tests
//! are scoped to the pure-domain JSON-to-types parse logic.

use podcast_core::migration::{from_state_json, MigrationError};
use podcast_core::{AutoDownloadMode, NostrVisibility, PodcastId, PodcastKind};

/// Minimal post-split fixture matching what `Persistence.write(_:)` emits
/// after `metadataState(from:)` strips episodes. Hand-built so the test is
/// robust against the legacy Swift encoder being unavailable in the Rust
/// test environment.
fn fixture_state_json() -> &'static str {
    r#"{
        "podcasts": [
            {
                "id": "11111111-1111-1111-1111-111111111111",
                "kind": "rss",
                "feedURL": "https://feeds.example.com/show.xml",
                "title": "Test Show",
                "author": "Test Author",
                "imageURL": "https://example.com/art.jpg",
                "description": "A show",
                "language": "en",
                "categories": ["Technology", "News"],
                "discoveredAt": "2025-01-15T10:30:00Z",
                "lastRefreshedAt": "2025-05-20T08:00:00Z",
                "etag": "W/\"abc123\"",
                "lastModified": "Tue, 20 May 2025 08:00:00 GMT",
                "titleIsPlaceholder": false,
                "nostrVisibility": "public"
            },
            {
                "id": "22222222-2222-2222-2222-222222222222",
                "kind": "synthetic",
                "title": "Agent Generated",
                "author": "",
                "description": "",
                "categories": [],
                "discoveredAt": "2025-02-01T00:00:00Z",
                "titleIsPlaceholder": false,
                "nostrVisibility": "private"
            }
        ],
        "subscriptions": [
            {
                "podcastID": "11111111-1111-1111-1111-111111111111",
                "subscribedAt": "2025-01-15T10:35:00Z",
                "autoDownload": {
                    "mode": { "allNew": {} },
                    "wifiOnly": true
                },
                "notificationsEnabled": true,
                "defaultPlaybackRate": 1.25
            }
        ]
    }"#
}

#[test]
fn parses_post_split_state_json() {
    let result = from_state_json(fixture_state_json().as_bytes()).expect("parse");
    // 2 real podcasts + the synthetic Unknown row injected on migration.
    assert_eq!(result.podcasts.len(), 3);

    let show = result
        .podcasts
        .iter()
        .find(|p| p.title == "Test Show")
        .expect("test show");
    assert_eq!(show.author, "Test Author");
    assert_eq!(show.categories, vec!["Technology", "News"]);
    assert_eq!(show.kind, PodcastKind::Rss);
    assert_eq!(show.nostr_visibility, NostrVisibility::Public);
    assert_eq!(
        show.feed_url.as_ref().map(|u| u.as_str()),
        Some("https://feeds.example.com/show.xml")
    );

    let synthetic = result
        .podcasts
        .iter()
        .find(|p| p.title == "Agent Generated")
        .expect("synthetic");
    assert_eq!(synthetic.kind, PodcastKind::Synthetic);
    assert_eq!(synthetic.nostr_visibility, NostrVisibility::Private);

    assert_eq!(result.subscriptions.len(), 1);
    let sub = &result.subscriptions[0];
    assert_eq!(
        sub.podcast_id.0.to_string(),
        "11111111-1111-1111-1111-111111111111"
    );
    assert!(sub.notifications_enabled);
    assert_eq!(sub.default_playback_rate, Some(1.25));
    assert_eq!(sub.auto_download.mode, AutoDownloadMode::AllNew);
    assert!(sub.auto_download.wifi_only);
}

#[test]
fn injects_unknown_podcast_row_if_absent() {
    let result = from_state_json(fixture_state_json().as_bytes()).expect("parse");
    assert!(
        result.podcasts.iter().any(|p| p.id == PodcastId::unknown()),
        "Podcast.unknownID sentinel row must exist post-migration so episodes with that FK resolve"
    );
}

#[test]
fn parses_latest_n_mode_with_underscore_zero_associated_value() {
    let json = r#"{
        "podcasts": [{
            "id": "33333333-3333-3333-3333-333333333333",
            "kind": "rss",
            "title": "X",
            "author": "",
            "description": "",
            "categories": [],
            "discoveredAt": "2025-03-01T00:00:00Z",
            "titleIsPlaceholder": false,
            "nostrVisibility": "public"
        }],
        "subscriptions": [{
            "podcastID": "33333333-3333-3333-3333-333333333333",
            "subscribedAt": "2025-03-01T00:00:00Z",
            "autoDownload": {
                "mode": { "latestN": { "_0": 7 } },
                "wifiOnly": false
            },
            "notificationsEnabled": false
        }]
    }"#;
    let result = from_state_json(json.as_bytes()).expect("parse");
    let sub = result.subscriptions.first().expect("subscription");
    assert_eq!(
        sub.auto_download.mode,
        AutoDownloadMode::LatestN { count: 7 }
    );
    assert!(!sub.auto_download.wifi_only);
    assert!(!sub.notifications_enabled);
}

#[test]
fn parses_off_mode() {
    let json = r#"{
        "subscriptions": [{
            "podcastID": "44444444-4444-4444-4444-444444444444",
            "subscribedAt": "2025-03-01T00:00:00Z",
            "autoDownload": {
                "mode": { "off": {} },
                "wifiOnly": true
            }
        }]
    }"#;
    let result = from_state_json(json.as_bytes()).expect("parse");
    let sub = result.subscriptions.first().expect("subscription");
    assert_eq!(sub.auto_download.mode, AutoDownloadMode::Off);
}

#[test]
fn empty_state_yields_only_unknown_podcast() {
    // A pristine app with no follows still gets the Unknown sentinel so
    // episode FKs to `Podcast.unknownID` resolve.
    let result = from_state_json(b"{}").expect("parse");
    assert_eq!(result.podcasts.len(), 1);
    assert_eq!(result.podcasts[0].id, PodcastId::unknown());
    assert!(result.subscriptions.is_empty());
}

#[test]
fn malformed_json_surfaces_as_error_not_panic() {
    // D6 — corrupt files don't crash; they surface a typed error the
    // shell can attach to the toast.
    let result = from_state_json(b"{not json");
    assert!(matches!(
        result,
        Err(MigrationError::MalformedStateJson(_))
    ));
}

#[test]
fn unknown_fields_in_legacy_blob_are_ignored() {
    // Forward-compat: a future Swift build may add new top-level keys.
    // Migration must keep working — that's why we don't use
    // `deny_unknown_fields`.
    let json = r#"{
        "podcasts": [],
        "subscriptions": [],
        "settings": { "anything": "goes" },
        "notes": [],
        "futureField": 42
    }"#;
    let result = from_state_json(json.as_bytes()).expect("parse");
    assert_eq!(result.podcasts.len(), 1); // just the Unknown row
}

#[test]
fn subscription_with_missing_optional_fields_decodes() {
    // `Persistence.swift` writes defensively but a hand-edited or older
    // file may omit any optional field; mirrors Swift's `decodeIfPresent`.
    let json = r#"{
        "subscriptions": [{
            "podcastID": "55555555-5555-5555-5555-555555555555",
            "subscribedAt": "2025-03-01T00:00:00Z"
        }]
    }"#;
    let result = from_state_json(json.as_bytes()).expect("parse");
    let sub = result.subscriptions.first().expect("subscription");
    assert!(sub.notifications_enabled, "default true");
    assert_eq!(sub.auto_download.mode, AutoDownloadMode::AllNew);
    assert!(sub.auto_download.wifi_only);
    assert!(sub.default_playback_rate.is_none());
}

#[test]
fn missing_feed_url_decodes_as_none() {
    // Synthetic podcasts (`.synthetic` kind) have nil `feedURL`. The
    // Swift encoder omits the key; the Rust decoder must accept that.
    let json = r#"{
        "podcasts": [{
            "id": "66666666-6666-6666-6666-666666666666",
            "kind": "synthetic",
            "title": "Synth",
            "author": "",
            "description": "",
            "categories": [],
            "discoveredAt": "2025-03-01T00:00:00Z",
            "titleIsPlaceholder": false,
            "nostrVisibility": "public"
        }]
    }"#;
    let result = from_state_json(json.as_bytes()).expect("parse");
    let synth = result
        .podcasts
        .iter()
        .find(|p| p.title == "Synth")
        .expect("synth");
    assert!(synth.feed_url.is_none());
}
