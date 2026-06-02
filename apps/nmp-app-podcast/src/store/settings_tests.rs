use super::*;
#[test]
fn fresh_store_has_default_skip_intervals() {
    let store = PodcastStore::new();
    assert!((store.skip_forward_secs() - 30.0).abs() < f64::EPSILON);
    assert!((store.skip_backward_secs() - 15.0).abs() < f64::EPSILON);
}
#[test]
fn set_skip_intervals_updates_values() {
    let mut store = PodcastStore::new();
    store.set_skip_intervals(45.0, 10.0);
    assert!((store.skip_forward_secs() - 45.0).abs() < f64::EPSILON);
    assert!((store.skip_backward_secs() - 10.0).abs() < f64::EPSILON);
}
#[test]
fn set_skip_intervals_clamps_to_bounds() {
    let mut store = PodcastStore::new();
    store.set_skip_intervals(0.0, 200.0);
    assert!((store.skip_forward_secs() - 1.0).abs() < f64::EPSILON);
    assert!((store.skip_backward_secs() - 120.0).abs() < f64::EPSILON);
}
#[test]
fn set_skip_intervals_same_value_is_noop() {
    let mut store = PodcastStore::new();
    // Writing defaults again must not change state
    store.set_skip_intervals(30.0, 15.0);
    assert!((store.skip_forward_secs() - 30.0).abs() < f64::EPSILON);
    assert!((store.skip_backward_secs() - 15.0).abs() < f64::EPSILON);
}
#[test]
fn fresh_store_effective_stt_provider_is_apple_native() {
    let store = PodcastStore::new();
    assert_eq!(store.stt_provider(), "apple_native");
    assert_eq!(store.effective_stt_provider(), "apple_native");
}
#[test]
fn effective_stt_provider_falls_back_without_key() {
    let mut store = PodcastStore::new();
    store.set_stt_provider("elevenlabs_scribe".to_owned());
    // No key reported yet → policy downgrades to apple_native.
    assert_eq!(store.effective_stt_provider(), "apple_native");
}
#[test]
fn effective_stt_provider_stays_selected_with_key() {
    let mut store = PodcastStore::new();
    store.set_stt_provider("elevenlabs_scribe".to_owned());
    store.set_stt_keys_present(vec!["elevenlabs_scribe".to_owned()]);
    assert_eq!(store.effective_stt_provider(), "elevenlabs_scribe");
}
#[test]
fn set_stt_keys_present_replaces_previous_set() {
    let mut store = PodcastStore::new();
    store.set_stt_keys_present(vec!["assemblyai".to_owned()]);
    assert!(store.stt_key_present("assemblyai"));
    // A fresh report omitting assemblyai must clear it (key was deleted).
    store.set_stt_keys_present(vec!["openrouter_whisper".to_owned()]);
    assert!(!store.stt_key_present("assemblyai"));
    assert!(store.stt_key_present("openrouter_whisper"));
}

// ── Settings round-trip (playback rate / auto-delete / auto-skip-ads) ────
//
// These exercise the store-accessor → `set_data_dir` reload path for the
// playback-shaped settings the task calls out. The `PersistedStore`-level
// field round-trips live in `store/persistence_tests.rs`; these confirm the
// public `PodcastStore` getters/setters hydrate correctly after a restart.

/// Local RAII tempdir — mirrors the helper pattern in `store/tests.rs` and
/// `ffi/audio_report_tests.rs`. Kept module-local so these settings tests
/// don't depend on the visibility of the `store::tests` helper.
struct TempDir {
    path: std::path::PathBuf,
}
impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nmp-settings-test-{}-{}",
            std::process::id(),
            n
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn fresh_store_default_playback_rate_is_one() {
    let store = PodcastStore::new();
    assert!((store.default_playback_rate() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn set_default_playback_rate_round_trips_in_memory() {
    let mut store = PodcastStore::new();
    store.set_default_playback_rate(1.5);
    assert!((store.default_playback_rate() - 1.5).abs() < f64::EPSILON);
}

#[test]
fn set_default_playback_rate_clamps_to_bounds() {
    let mut store = PodcastStore::new();
    // Above the 3.0 upper bound clamps to 3.0.
    store.set_default_playback_rate(5.0);
    assert!((store.default_playback_rate() - 3.0).abs() < f64::EPSILON);
    // Below the 0.5 lower bound clamps to 0.5.
    store.set_default_playback_rate(0.1);
    assert!((store.default_playback_rate() - 0.5).abs() < f64::EPSILON);
}

#[test]
fn default_playback_rate_persists_across_reload() {
    let dir = TempDir::new();
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        store.set_default_playback_rate(1.75);
    }
    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    assert!((store2.default_playback_rate() - 1.75).abs() < f64::EPSILON);
}

#[test]
fn fresh_store_auto_delete_after_played_is_false() {
    let store = PodcastStore::new();
    assert!(!store.auto_delete_downloads_after_played());
}

#[test]
fn set_auto_delete_after_played_round_trips_in_memory() {
    let mut store = PodcastStore::new();
    store.set_auto_delete_downloads_after_played(true);
    assert!(store.auto_delete_downloads_after_played());
    store.set_auto_delete_downloads_after_played(false);
    assert!(!store.auto_delete_downloads_after_played());
}

#[test]
fn auto_delete_after_played_persists_across_reload() {
    let dir = TempDir::new();
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        store.set_auto_delete_downloads_after_played(true);
    }
    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    assert!(store2.auto_delete_downloads_after_played());
}

#[test]
fn auto_skip_ads_persists_through_store_reload() {
    // The `PersistedStore`-level round-trip is covered in
    // `persistence_tests.rs`; this asserts the public store accessor path
    // (`set_auto_skip_ads_enabled` → `set_data_dir` reload) end to end.
    let dir = TempDir::new();
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        assert!(!store.auto_skip_ads_enabled(), "defaults off");
        store.set_auto_skip_ads_enabled(true);
    }
    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    assert!(store2.auto_skip_ads_enabled());
}

