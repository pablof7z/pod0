//! Bridge-decoder fixture tests for `WidgetSnapshot` and `HandoffState`.
//!
//! These tests prove the Rust-emitted JSON key names are exactly what
//! Swift's `.convertFromSnakeCase` strategy converts to the camelCase
//! property names declared in `PodcastPlatformTypes.generated.swift`.
//!
//! ## Why these tests, not just round-trips?
//!
//! A Rust `WidgetSnapshot` round-trip (`serde_json::to_string` →
//! `serde_json::from_str`) passes even when the wire keys are wrong, because
//! Rust serde always uses the snake_case Rust field names and does not care
//! about Swift's camelCase mapping.  The real failure mode — documented in
//! PRs #366 and #371 — only manifests when the Swift bridge decoder applies
//! `.convertFromSnakeCase` to the Rust-emitted JSON.  These tests pin the
//! exact wire keys so any accidental rename in the Rust struct (e.g. adding
//! an explicit `#[serde(rename = "...")]` that conflicts with what Swift
//! expects) fails here instead of in production.
//!
//! `HandoffState` is constructed in Swift, not decoded from JSON via the
//! bridge decoder.  Its fixture test verifies the Rust `serde_json` output
//! uses snake_case keys that match the explicit `CodingKeys` overrides in the
//! generated Swift struct (`episode_id`, `podcast_id`, `activity_type`, etc.).

use crate::ffi::projections::WidgetSnapshot;
use podcast_core::types::HandoffState;

// ── WidgetSnapshot ────────────────────────────────────────────────────────────

/// Pin every wire key that Swift's `.convertFromSnakeCase` strategy must map.
///
/// The Swift generated struct (`PodcastPlatformTypes.generated.swift`) declares:
///   `var nowPlayingEpisodeTitle`  ← from `now_playing_episode_title`
///   `var nowPlayingPodcastTitle`  ← from `now_playing_podcast_title`
///   `var nowPlayingArtworkUrl`    ← from `now_playing_artwork_url`
///   `var nowPlayingChapterTitle`  ← from `now_playing_chapter_title`
///   `var isPlaying`               ← from `is_playing`
///   `var positionFraction`        ← from `position_fraction`
///   `var positionSecs`            ← from `position_secs`
///   `var durationSecs`            ← from `duration_secs`
///   `var unplayedCount`           ← from `unplayed_count`
///
/// Acronym rule: `.convertFromSnakeCase` lowercases every word component,
/// so `url` stays lowercase and produces `artworkUrl` NOT `artworkURL`.
/// The test asserts the exact snake_case key name to catch any Rust rename.
#[test]
fn widget_snapshot_wire_keys_match_swift_bridge_contract() {
    let widget = WidgetSnapshot {
        now_playing_episode_title: Some("Ep 42".into()),
        now_playing_podcast_title: Some("Some Show".into()),
        now_playing_artwork_url: Some("https://ex.com/art.png".into()),
        now_playing_chapter_title: Some("Chapter 2".into()),
        is_playing: true,
        position_fraction: 0.5,
        position_secs: 600.0,
        duration_secs: 1200.0,
        unplayed_count: 3,
    };
    let json = serde_json::to_string(&widget).expect("encode");

    // Each assertion pins ONE wire key.  `.convertFromSnakeCase` converts the
    // key on the right into the Swift property name shown in the comment.
    assert!(json.contains(r#""now_playing_episode_title""#),
        "must emit now_playing_episode_title → Swift nowPlayingEpisodeTitle");
    assert!(json.contains(r#""now_playing_podcast_title""#),
        "must emit now_playing_podcast_title → Swift nowPlayingPodcastTitle");
    assert!(json.contains(r#""now_playing_artwork_url""#),
        "must emit now_playing_artwork_url → Swift nowPlayingArtworkUrl (NOT artworkURL)");
    assert!(json.contains(r#""now_playing_chapter_title""#),
        "must emit now_playing_chapter_title → Swift nowPlayingChapterTitle");
    assert!(json.contains(r#""is_playing":true"#),
        "must emit is_playing → Swift isPlaying");
    assert!(json.contains(r#""position_fraction""#),
        "must emit position_fraction → Swift positionFraction");
    assert!(json.contains(r#""position_secs""#),
        "must emit position_secs → Swift positionSecs");
    assert!(json.contains(r#""duration_secs""#),
        "must emit duration_secs → Swift durationSecs");
    assert!(json.contains(r#""unplayed_count":3"#),
        "must emit unplayed_count → Swift unplayedCount");

    // Negative: no camelCase keys must appear — those would indicate a Rust
    // `#[serde(rename = "...")]` that conflicts with the bridge strategy.
    assert!(!json.contains("nowPlaying"),
        "Rust JSON must NOT contain camelCase keys; those break the bridge decoder");
    assert!(!json.contains("isPlaying"),
        "Rust JSON must NOT contain camelCase isPlaying");
    assert!(!json.contains("positionFraction"),
        "Rust JSON must NOT contain camelCase positionFraction");
    assert!(!json.contains("artworkURL"),
        "Rust JSON must NOT emit artworkURL — Swift expects artworkUrl (lowercase 'rl')");
}

/// Verify that a `WidgetSnapshot` with all optional fields absent still emits
/// the required fields with snake_case keys and omits the None optionals (D5).
#[test]
fn widget_snapshot_minimal_wire_keys_snake_case_required_only() {
    let widget = WidgetSnapshot {
        now_playing_episode_title: None,
        now_playing_podcast_title: None,
        now_playing_artwork_url: None,
        now_playing_chapter_title: None,
        is_playing: false,
        position_fraction: 0.0,
        position_secs: 0.0,
        duration_secs: 0.0,
        unplayed_count: 0,
    };
    let json = serde_json::to_string(&widget).expect("encode");

    // Required (always-present) fields use snake_case.
    assert!(json.contains(r#""is_playing":false"#));
    assert!(json.contains(r#""position_fraction""#));
    assert!(json.contains(r#""position_secs""#));
    assert!(json.contains(r#""duration_secs""#));
    assert!(json.contains(r#""unplayed_count":0"#));

    // Optional fields absent when None (D5 `skip_serializing_if = "is_none"`).
    assert!(!json.contains("now_playing_episode_title"),
        "D5: absent when None");
    assert!(!json.contains("now_playing_artwork_url"),
        "D5: absent when None");
}

// ── HandoffState ─────────────────────────────────────────────────────────────

/// Pin the wire keys for `HandoffState` — playing variant.
///
/// The Swift generated struct declares explicit `CodingKeys` overrides:
///   `case activityType = "activity_type"`
///   `case episodeID    = "episode_id"`
///   `case podcastID    = "podcast_id"`
///   `case positionSecs = "position_secs"`
///
/// These overrides are required because `.convertFromSnakeCase` would map
/// `episode_id` → `episodeId` (lowercase d), while the Swift property is
/// `episodeID` (uppercase D). The fixture confirms Rust emits the exact
/// snake_case keys the `CodingKeys` overrides expect.
#[test]
fn handoff_state_playing_wire_keys_match_swift_coding_key_overrides() {
    let state = HandoffState::playing("ep-fixture-1", 123.5);
    let json = serde_json::to_string(&state).expect("encode");

    // Required fields for the playing variant.
    assert!(json.contains(r#""activity_type":"io.f7z.podcast.playing""#),
        "must emit activity_type → Swift CodingKey activityType = \"activity_type\"");
    assert!(json.contains(r#""episode_id":"ep-fixture-1""#),
        "must emit episode_id → Swift CodingKey episodeID = \"episode_id\"");
    assert!(json.contains(r#""position_secs":123.5"#),
        "must emit position_secs → Swift CodingKey positionSecs = \"position_secs\"");

    // Optional fields absent when None — D5 `skip_serializing_if = "is_none"`.
    assert!(!json.contains("podcast_id"),
        "podcast_id must be absent for the playing activity (None → omitted)");

    // No camelCase keys — these would silently bypass the CodingKey overrides.
    assert!(!json.contains("activityType"),
        "Rust must NOT emit camelCase activityType");
    assert!(!json.contains("episodeID"),
        "Rust must NOT emit camelCase episodeID; wire key is episode_id");
    assert!(!json.contains("positionSecs"),
        "Rust must NOT emit camelCase positionSecs");
}

/// Pin wire keys for `HandoffState` — browsing variant.
#[test]
fn handoff_state_browsing_wire_keys_match_swift_coding_key_overrides() {
    let state = HandoffState::browsing_podcast("pod-fixture-1");
    let json = serde_json::to_string(&state).expect("encode");

    assert!(json.contains(r#""activity_type":"io.f7z.podcast.browsing""#),
        "must emit activity_type for browsing");
    assert!(json.contains(r#""podcast_id":"pod-fixture-1""#),
        "must emit podcast_id → Swift CodingKey podcastID = \"podcast_id\"");

    // Fields absent when None.
    assert!(!json.contains("episode_id"),
        "episode_id must be absent for the browsing activity");
    assert!(!json.contains("position_secs"),
        "position_secs must be absent for the browsing activity");

    // No camelCase keys.
    assert!(!json.contains("podcastID"),
        "Rust must NOT emit camelCase podcastID; wire key is podcast_id");
}
