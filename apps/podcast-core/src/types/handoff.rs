//! Handoff (`NSUserActivity`) activity-type identifiers + a typed
//! state record the M11 platform capability translates into the
//! actual `NSUserActivity` the iOS shell donates.
//!
//! The legacy iOS app used a string `HandoffActivityType` enum
//! (`App/Sources/Services/HandoffActivityType.swift`); the NMP
//! migration moves the *decision* of which activity to surface,
//! and what user-info to carry, into Rust so cross-device
//! continuation is identical on every platform that ever grows
//! a Handoff equivalent (macOS, iPadOS, Apple Watch).
//!
//! Per D7 the iOS executor (`PlatformCapability.updateHandoff(...)`)
//! converts a [`HandoffState`] into an `NSUserActivity` and donates
//! it; it never chooses *whether* to donate.
//!
//! ## Activity-type ids
//!
//! The string ids follow the reverse-DNS convention iOS expects in
//! `Info.plist`'s `NSUserActivityTypes`. They are bundle-scoped to
//! `io.f7z.podcast`, matching the production bundle identifier
//! (per the project memory: `Bundle ID is io.f7z.podcast, NOT
//! com.podcastr.app — that's the App Group`).

use serde::{Deserialize, Serialize};

/// Activity type for "user is listening to an episode" — donated
/// while playback is active. Receiving devices can resume at the
/// reported `position_secs`.
pub const HANDOFF_ACTIVITY_PLAYING: &str = "io.f7z.podcast.playing";

/// Activity type for "user is browsing the library / a show" —
/// donated while a non-player surface is foregrounded. Receiving
/// devices open the same surface.
pub const HANDOFF_ACTIVITY_BROWSING: &str = "io.f7z.podcast.browsing";

/// Typed state the kernel emits on the snapshot when Handoff
/// should be surfaced. The iOS capability translates this into
/// an `NSUserActivity` with the corresponding `activityType` and
/// `userInfo` keys.
///
/// `activity_type` is stored as `String` rather than `&'static str`
/// so the type round-trips through `serde_json::from_str(&owned)`
/// without imposing a `'static` lifetime on the JSON input. The
/// iOS executor validates the received string against the known
/// set (`HANDOFF_ACTIVITY_PLAYING` / `HANDOFF_ACTIVITY_BROWSING`)
/// before donating.
///
/// `episode_id` / `podcast_id` are surfaced as `Option<String>`
/// rather than typed ids so the JSON payload stays portable
/// (iOS receivers may not have the same id type available yet
/// when they're cold-launched from a Handoff continuation).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HandoffState {
    pub activity_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub podcast_id: Option<String>,
    /// Position in seconds, when the activity is `playing`. The
    /// receiver seeks to this on `restoreUserActivityState`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_secs: Option<f64>,
}

impl HandoffState {
    /// Build a `playing` handoff state for the given episode +
    /// position.
    pub fn playing(episode_id: impl Into<String>, position_secs: f64) -> Self {
        Self {
            activity_type: HANDOFF_ACTIVITY_PLAYING.to_owned(),
            episode_id: Some(episode_id.into()),
            podcast_id: None,
            position_secs: Some(position_secs),
        }
    }

    /// Build a `browsing` handoff state for the given podcast.
    pub fn browsing_podcast(podcast_id: impl Into<String>) -> Self {
        Self {
            activity_type: HANDOFF_ACTIVITY_BROWSING.to_owned(),
            episode_id: None,
            podcast_id: Some(podcast_id.into()),
            position_secs: None,
        }
    }

    /// `true` when `activity_type` matches one of the known
    /// platform-capability activity ids. The iOS executor calls
    /// this before donating so an unknown payload (e.g. a future
    /// activity id encoded by a newer kernel) is dropped rather
    /// than donated with a string the receiver can't route.
    pub fn is_known_activity(&self) -> bool {
        matches!(
            self.activity_type.as_str(),
            HANDOFF_ACTIVITY_PLAYING | HANDOFF_ACTIVITY_BROWSING
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_type_ids_match_documented_strings() {
        assert_eq!(HANDOFF_ACTIVITY_PLAYING, "io.f7z.podcast.playing");
        assert_eq!(HANDOFF_ACTIVITY_BROWSING, "io.f7z.podcast.browsing");
    }

    #[test]
    fn playing_constructor_populates_episode_and_position() {
        let state = HandoffState::playing("ep-1", 42.5);
        assert_eq!(state.activity_type, HANDOFF_ACTIVITY_PLAYING);
        assert_eq!(state.episode_id.as_deref(), Some("ep-1"));
        assert_eq!(state.position_secs, Some(42.5));
        assert!(state.podcast_id.is_none());
        assert!(state.is_known_activity());
    }

    #[test]
    fn browsing_constructor_populates_podcast() {
        let state = HandoffState::browsing_podcast("pod-1");
        assert_eq!(state.activity_type, HANDOFF_ACTIVITY_BROWSING);
        assert_eq!(state.podcast_id.as_deref(), Some("pod-1"));
        assert!(state.episode_id.is_none());
        assert!(state.position_secs.is_none());
        assert!(state.is_known_activity());
    }

    #[test]
    fn is_known_activity_rejects_unknown_string() {
        let state = HandoffState {
            activity_type: "io.f7z.podcast.future_activity".to_owned(),
            episode_id: None,
            podcast_id: None,
            position_secs: None,
        };
        assert!(!state.is_known_activity());
    }

    #[test]
    fn handoff_state_round_trips_through_json() {
        let state = HandoffState::playing("ep-1", 12.0);
        let json = serde_json::to_string(&state).expect("encode");
        let decoded: HandoffState = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, state);
    }

    #[test]
    fn handoff_state_omits_none_fields_in_json() {
        let state = HandoffState::browsing_podcast("pod-1");
        let json = serde_json::to_string(&state).expect("encode");
        assert!(!json.contains("episode_id"));
        assert!(!json.contains("position_secs"));
        assert!(json.contains("podcast_id"));
        assert!(json.contains("activity_type"));
    }
}
