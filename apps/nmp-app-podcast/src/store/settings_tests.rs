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

