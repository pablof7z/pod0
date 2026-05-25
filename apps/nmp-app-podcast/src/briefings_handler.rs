//! Briefing-specific host op handlers.
//!
//! Split out of [`crate::host_op_handler`] so the M9 briefing surface
//! (composer dispatch, scheduler updates, briefing snapshot building)
//! has a dedicated home as it grows in M9.B. M9.A only needs the
//! generate-briefing stub; subsequent milestones land here.
//!
//! Per D0 / D6: every entry point returns an in-band JSON envelope so
//! callers can dispatch without a back-channel for failures.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::ffi::projections::BriefingSnapshot;

/// M9.A stub for the `podcast.generate_briefing` action.
///
/// Flips the snapshot's briefing slot into the `"generating"` lifecycle
/// state so the iOS Briefings tab immediately renders the "Composing your
/// briefing" placeholder and stops showing the empty-state CTA on the
/// next snapshot poll. The full composer + audio stitching chain (LLM
/// segment drafting, ElevenLabs TTS, AVAudioEngine stitching,
/// persistence) lands in M9.B and will replace this stub's body with the
/// real composer dispatch + completion flow.
///
/// Returns the standard `{"ok": true, "status": "generating"}` envelope.
/// Bumps `rev` so the iOS snapshot poll picks up the slot change on its
/// next tick (without bumping rev the polled snapshot's `rev` would not
/// advance and iOS would skip the update).
///
/// Per D6 the response is always an in-band envelope; the iOS view
/// surfaces `status` as the optimistic UI hint.
pub fn handle_generate_briefing(
    slot: &Arc<Mutex<Option<BriefingSnapshot>>>,
    rev: &Arc<AtomicU64>,
) -> serde_json::Value {
    if let Ok(mut s) = slot.lock() {
        *s = Some(BriefingSnapshot {
            status: "generating".to_owned(),
            is_generating: true,
            segment_count: 0,
            segments: Vec::new(),
            last_generated_at: None,
            next_scheduled_minutes: None,
        });
        rev.fetch_add(1, Ordering::Relaxed);
    }
    serde_json::json!({"ok": true, "status": "generating"})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_returns_ok_envelope_with_generating_status() {
        let slot = Arc::new(Mutex::new(None));
        let rev = Arc::new(AtomicU64::new(0));
        let v = handle_generate_briefing(&slot, &rev);
        assert_eq!(v["ok"], true);
        assert_eq!(v["status"], "generating");
    }

    #[test]
    fn stub_writes_generating_snapshot_and_bumps_rev() {
        let slot = Arc::new(Mutex::new(None));
        let rev = Arc::new(AtomicU64::new(7));
        let _ = handle_generate_briefing(&slot, &rev);
        let stored = slot.lock().unwrap().clone().expect("briefing slot populated");
        assert_eq!(stored.status, "generating");
        assert!(stored.is_generating);
        assert!(stored.segments.is_empty());
        assert_eq!(rev.load(Ordering::Relaxed), 8);
    }
}
