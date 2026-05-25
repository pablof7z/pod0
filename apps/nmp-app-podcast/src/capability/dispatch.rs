//! Pure JSON ↔ JSON bridge between the iOS audio capability and the
//! Rust [`crate::player::PlayerActor`].
//!
//! This is the seam M3.B will plug into the kernel-side `ActionModule`
//! and `CapabilityModule` registrations. Today it isolates the JSON
//! envelope handling from the actor itself so:
//!
//! 1. The actor stays a pure state machine (`PlayerActor::handle_audio_report`
//!    takes a typed `AudioReport`, not a string), keeping the unit tests
//!    cheap and the surface narrow.
//! 2. The kernel-side `ActionModule` (M3.B) and the iOS-side
//!    `PodcastCapabilities.handleJSON` router will all funnel through
//!    these helpers so the JSON shapes don't drift across the four
//!    layers (Swift encoder → C-ABI → Rust decoder → projection).
//!
//! D7 holds at every step: the helpers parse, project, and re-encode;
//! they never inspect content to make a playback decision. All decisions
//! live in [`crate::player::PlayerActor`].

use std::time::SystemTime;

use crate::capability::{AudioCommand, AudioReport};
use crate::player::PlayerActor;

/// Outcome of feeding a JSON-encoded [`AudioReport`] into a
/// [`PlayerActor`].
#[derive(Debug)]
pub enum DispatchOutcome {
    /// The report decoded and projected; `follow_up_json` is the JSON
    /// of the [`AudioCommand`] the kernel should hand back to the
    /// capability (`None` when no command is needed).
    Ok { follow_up_json: Option<String> },
    /// The inbound JSON couldn't be decoded as an [`AudioReport`].
    /// Per D6 this is data, not an exception — the caller decides
    /// whether to log, drop, or surface to diagnostics.
    DecodeFailed { error: String },
}

/// Decode a JSON-encoded [`AudioReport`], apply it to `actor`, and
/// return the follow-up [`AudioCommand`] (if any) as JSON ready to send
/// back to the iOS capability.
///
/// Errors degrade to [`DispatchOutcome::DecodeFailed`] — D6: no panics,
/// no `Result` leaking across the layer boundary in a position where the
/// caller can't recover.
pub fn dispatch_audio_report_json(
    actor: &mut PlayerActor,
    report_json: &str,
    now: SystemTime,
) -> DispatchOutcome {
    let report: AudioReport = match serde_json::from_str(report_json) {
        Ok(r) => r,
        Err(err) => {
            return DispatchOutcome::DecodeFailed {
                error: err.to_string(),
            }
        }
    };

    let follow_up = actor.handle_audio_report(report, now);
    let follow_up_json = follow_up.and_then(|cmd| serde_json::to_string(&cmd).ok());
    DispatchOutcome::Ok { follow_up_json }
}

/// Encode an [`AudioCommand`] for the iOS capability. Returns `None`
/// on the (impossible) serde failure — the caller treats `None` as
/// "no-op", which is the safest D6 fall-back for an outbound command.
#[must_use]
pub fn encode_audio_command(cmd: &AudioCommand) -> Option<String> {
    serde_json::to_string(cmd).ok()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn t0() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_700_000_000)
    }

    #[test]
    fn playing_report_json_round_trip_no_follow_up() {
        let mut actor = PlayerActor::new();
        let report = r#"{"type":"playing","url":"u","position_secs":1.0,"duration_secs":10.0}"#;
        let outcome = dispatch_audio_report_json(&mut actor, report, t0());
        match outcome {
            DispatchOutcome::Ok { follow_up_json } => assert!(follow_up_json.is_none()),
            DispatchOutcome::DecodeFailed { error } => panic!("decode failed: {error}"),
        }
        assert!(actor.state().is_playing);
        assert_eq!(actor.state().position_secs, 1.0);
    }

    #[test]
    fn sleep_timer_fired_emits_stop_command_json() {
        let mut actor = PlayerActor::new();
        actor.arm_sleep_timer(Duration::from_secs(60), t0());
        let outcome =
            dispatch_audio_report_json(&mut actor, r#"{"type":"sleep_timer_fired"}"#, t0());
        match outcome {
            DispatchOutcome::Ok { follow_up_json } => {
                assert_eq!(follow_up_json.as_deref(), Some(r#"{"type":"stop"}"#));
            }
            DispatchOutcome::DecodeFailed { error } => panic!("decode failed: {error}"),
        }
    }

    #[test]
    fn malformed_report_returns_decode_failed() {
        let mut actor = PlayerActor::new();
        let outcome = dispatch_audio_report_json(&mut actor, "not-json", t0());
        assert!(matches!(outcome, DispatchOutcome::DecodeFailed { .. }));
        // Actor state untouched on a decode failure.
        assert!(!actor.state().is_playing);
    }

    #[test]
    fn encode_audio_command_round_trips() {
        let cmd = AudioCommand::seek(99.0);
        let json = encode_audio_command(&cmd).expect("encode");
        let decoded: AudioCommand = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, cmd);
    }
}
