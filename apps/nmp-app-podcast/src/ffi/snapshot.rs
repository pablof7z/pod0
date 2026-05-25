//! Snapshot + unregister entry points the host calls against a
//! [`PodcastHandle`] returned by [`super::register::nmp_app_podcast_register`].
//!
//! ## `PodcastUpdate`
//!
//! [`PodcastUpdate`] is the typed root of the JSON the kernel emits on every
//! tick. The iOS shell decodes it via `Codable`. Fields are added milestone by
//! milestone (see `Plans/nmp-migration/04-snapshot.md` for the full target
//! shape).
//!
//! For M3.A the only new field is `now_playing: Option<PlayerState>`. Every
//! other field stays unset until its milestone lands — the empty defaults are
//! deliberately byte-compatible with the legacy stub payload
//! (`{"running":true,"rev":0,"schema_version":1}`) so existing decoders don't
//! break before M3.B wires the player projection.

use std::ffi::{c_char, CString};

use serde::{Deserialize, Serialize};

use super::handle::PodcastHandle;
use crate::player::PlayerState;

/// Typed root of the snapshot JSON.
///
/// `running`, `rev`, and `schema_version` mirror the kernel's existing
/// tick contract. `now_playing` lands at M3.A; subsequent milestones add
/// more fields (`podcasts`, `today_queue`, `triage`, …) per
/// `Plans/nmp-migration/04-snapshot.md`.
///
/// Forward compatibility: Swift's `Codable` round-trip tolerates unknown
/// fields, so introducing a new field here only needs a matching Swift
/// decoder. **Backward** compatibility (older binaries decoding a newer
/// snapshot) is the contract behind `schema_version`; bump it only when
/// removing or renaming a field.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PodcastUpdate {
    /// `true` once the kernel is running. False during shutdown.
    pub running: bool,
    /// Monotonically increasing revision id; iOS uses it to dedupe ticks.
    pub rev: u64,
    /// Schema version — bump on incompatible shape changes.
    pub schema_version: u32,
    /// Active player projection, or `None` when nothing is loaded.
    ///
    /// Per D5 the field is `null` when no episode is loaded so the
    /// iOS decoder doesn't render a hero with default zeros.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_playing: Option<PlayerState>,
}

impl Default for PodcastUpdate {
    fn default() -> Self {
        Self {
            running: true,
            rev: 0,
            schema_version: 1,
            now_playing: None,
        }
    }
}

/// Build the JSON payload the FFI snapshot function returns. Extracted so
/// future milestones can hook into the same `PodcastUpdate` value (set
/// `now_playing` from `PlayerActor::state()`, populate `podcasts`, etc.)
/// without re-touching the C-ABI boundary.
fn build_snapshot_payload() -> String {
    // Build via the typed struct so renames stay one-and-done. Falls back
    // to the byte-compatible legacy stub on the (impossible) serde failure,
    // preserving D6.
    serde_json::to_string(&PodcastUpdate::default())
        .unwrap_or_else(|_| r#"{"running":true,"rev":0,"schema_version":1}"#.to_owned())
}

/// Serialize the current app state into a JSON C string.
///
/// Returns null on any failure (null handle, `CString` nul-byte conflict).
/// The returned pointer is owned by the caller; pass it to
/// [`nmp_app_podcast_snapshot_free`] when done.
///
/// The payload shape is defined by [`PodcastUpdate`]; new projections are
/// added milestone by milestone (M3.A adds `now_playing`; subsequent
/// milestones wire `podcasts`, `today_queue`, `triage`, …).
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_snapshot(handle: *mut PodcastHandle) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller guarantees `handle` is a valid pointer returned by
    // `nmp_app_podcast_register` and not yet freed.
    let _handle = unsafe { &*handle };

    let payload = build_snapshot_payload();
    let Ok(cstr) = CString::new(payload) else {
        return std::ptr::null_mut();
    };
    cstr.into_raw()
}

/// Free a snapshot string previously returned by [`nmp_app_podcast_snapshot`].
/// Null pointer is a silent no-op.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_snapshot_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: caller guarantees `ptr` came from `CString::into_raw` in
    // `nmp_app_podcast_snapshot` and has not been freed.
    unsafe {
        let _ = CString::from_raw(ptr);
    }
}

/// Drop the handle and free associated resources.
/// Idempotent: null pointer is a silent no-op. The handle MUST NOT be used
/// after this call.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_unregister(handle: *mut PodcastHandle) {
    if handle.is_null() {
        return;
    }
    // SAFETY: caller guarantees `handle` came from `nmp_app_podcast_register`
    // and has not already been freed.
    let boxed = unsafe { Box::from_raw(handle) };
    // Future milestones will use `boxed.app` to call
    // `app_ref.unregister_event_observer(observer_id)` for each registered
    // projection. For now the handle carries the `app` pointer so subsequent
    // milestones can add unregister logic here without changing the FFI type.
    let _ = boxed.app;
    // boxed dropped here.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_snapshot_omits_now_playing() {
        let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
        // `skip_serializing_if = "Option::is_none"` keeps the empty
        // payload byte-identical to the legacy stub.
        assert_eq!(json, r#"{"running":true,"rev":0,"schema_version":1}"#);
    }

    #[test]
    fn snapshot_with_now_playing_round_trips() {
        let mut state = PlayerState::idle();
        state.episode_id = Some("ep-1".into());
        state.url = Some("https://ex.com/ep-1.mp3".into());
        state.position_secs = 12.0;
        state.is_playing = true;

        let snap = PodcastUpdate {
            now_playing: Some(state.clone()),
            ..PodcastUpdate::default()
        };
        let json = serde_json::to_string(&snap).expect("encode");
        let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.now_playing, Some(state));
        assert!(decoded.running);
        assert_eq!(decoded.schema_version, 1);
    }

    #[test]
    fn build_snapshot_payload_is_valid_json() {
        let payload = build_snapshot_payload();
        let _decoded: PodcastUpdate = serde_json::from_str(&payload).expect("decode");
    }

    #[test]
    fn snapshot_decoder_tolerates_unknown_fields() {
        // Forward-compat: an older binary decoding a newer snapshot ignores
        // fields it doesn't know about (Codable parity).
        let payload = r#"{"running":true,"rev":7,"schema_version":1,"future_field":"ignored"}"#;
        let decoded: PodcastUpdate = serde_json::from_str(payload).expect("decode");
        assert_eq!(decoded.rev, 7);
        assert!(decoded.now_playing.is_none());
    }
}
