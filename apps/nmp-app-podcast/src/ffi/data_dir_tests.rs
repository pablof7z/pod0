use super::*;
use crate::download::DownloadQueue;
use crate::ffi::handle::PodcastHandle;
use crate::ffi::projections::AgentTaskSummary;
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::store::identity::IdentityStore;
use crate::store::PodcastStore;
use std::collections::HashSet;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
/// Build a `PodcastHandle` with a NULL `app` pointer — these tests only
/// exercise the data-dir path, which never touches `app`.
fn make_handle(store: Arc<Mutex<PodcastStore>>, rev: Arc<AtomicU64>) -> Box<PodcastHandle> {
    use std::collections::HashMap;
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    let state = Arc::new(crate::state::PodcastAppState::new_with_identity(
        crate::state::Infra::for_test(),
        store.clone(),
        identity.clone(),
    ));
    Box::new(PodcastHandle {
        app: std::ptr::null_mut(),
        state,
        player_actor: Arc::new(Mutex::new(PlayerActor::new())),
        store: store.clone(),
        identity,
        rev: rev.clone(),
        snapshot_signal: None,
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(HashMap::new())),
        queue: Arc::new(Mutex::new(PlaybackQueue::new())),
        download_queue: Arc::new(Mutex::new(DownloadQueue::new())),
        // clips, transcripts, agent_tasks removed in Steps 5a, 5b, 6 —
        // now owned by state.clips / state.transcripts / state.tasks.
        // search_results, nostr_results, comments_cache, viewed_comments_episode_id,
        // social, agent_notes removed in Steps 8-10 — now owned by
        // state.discovery / state.comments / state.social.
        dismissed_episode_ids: Arc::new(Mutex::new(HashSet::new())),
        // podcast_keys and publish_state removed in Step 13 —
        // now owned by state.publish (PublishState).
        // voice_state and voice_conversation removed in Step 12 —
        // now owned by state.voice (VoiceSubstate).
        // conversation, agent_busy, agent_touched removed in Step 11 —
        // now owned by state.agent_chat.
        inbox_triage_cache: Arc::new(Mutex::new(HashMap::new())),
        inbox_triage_in_progress: Arc::new(AtomicBool::new(false)),
        feedback: nmp_feedback::FeedbackRuntime::new(
            nmp_feedback::FeedbackConfig::new(crate::PODCAST_FEEDBACK_PROJECT_COORDINATE)
                .with_interest_namespace(crate::PODCAST_FEEDBACK_INTEREST_NAMESPACE),
            Arc::new(Mutex::new(Vec::new())),
            rev.clone(),
        ),
        runtime: Arc::new(tokio::runtime::Runtime::new().unwrap()),
        feed_fetch: crate::feed_fetch::FeedFetchCoordinator::new_test(),
    })
}
struct TempDir {
    path: PathBuf,
}
impl TempDir {
    fn new(tag: &str) -> Self {
        use std::sync::atomic::AtomicU64;
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nmp-podcast-ffi-{}-{}-{}",
            tag,
            std::process::id(),
            n,
        ));
        std::fs::create_dir_all(&path).expect("create tempdir");
        Self { path }
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
#[test]
fn null_handle_is_silent_noop() {
    let path = CString::new("/tmp/whatever").unwrap();
    nmp_app_podcast_set_data_dir(std::ptr::null_mut(), path.as_ptr());
    // Did not crash — D6 satisfied.
}
#[test]
fn null_path_is_silent_noop() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let handle = make_handle(store.clone(), rev.clone());
    let ptr = Box::into_raw(handle);
    nmp_app_podcast_set_data_dir(ptr, std::ptr::null());
    assert!(store.lock().unwrap().data_dir().is_none());
    assert_eq!(rev.load(Ordering::Relaxed), 0);
    // SAFETY: we boxed it ourselves above.
    let _ = unsafe { Box::from_raw(ptr) };
}
#[test]
fn binds_data_dir_and_does_not_bump_rev_when_empty() {
    let dir = TempDir::new("bind");
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let handle = make_handle(store.clone(), rev.clone());
    let ptr = Box::into_raw(handle);
    let cpath = CString::new(dir.path.to_str().unwrap()).unwrap();
    nmp_app_podcast_set_data_dir(ptr, cpath.as_ptr());
    assert!(store.lock().unwrap().data_dir().is_some());
    // No file exists yet, so nothing was loaded — rev should stay put.
    assert_eq!(rev.load(Ordering::Relaxed), 0);
    let _ = unsafe { Box::from_raw(ptr) };
}
#[test]
fn relay_sidecar_present_with_null_app_is_silent_noop() {
    // A persisted relay sidecar must NOT cause a null-pointer deref when the
    // handle has no app (D6). The real seam (`set_initial_relays_for_start`)
    // is exercised in the FFI smoke path; here we only assert robustness: a
    // present sidecar + null app binds the data dir and does not crash.
    let dir = TempDir::new("relay-noop");
    crate::store::relay_config::save_relay_config(
        &dir.path,
        &[("wss://saved.example".to_string(), "both".to_string())],
    )
    .expect("seed sidecar");
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let handle = make_handle(store.clone(), rev.clone());
    let ptr = Box::into_raw(handle);
    let cpath = CString::new(dir.path.to_str().unwrap()).unwrap();
    // app is null in the test handle — must not deref it.
    nmp_app_podcast_set_data_dir(ptr, cpath.as_ptr());
    assert!(store.lock().unwrap().data_dir().is_some());
    let _ = unsafe { Box::from_raw(ptr) };
}

#[test]
fn relay_sidecar_round_trips_via_load_helper() {
    // The data-dir directory the FFI binds is exactly the directory the
    // host-op handler writes the sidecar into, so a write-then-load round
    // trips through the same path. This guards the file-location contract
    // between the save (handler) and load (this FFI) halves.
    let dir = TempDir::new("relay-roundtrip");
    let relays = vec![
        ("wss://a.example".to_string(), "read".to_string()),
        ("wss://b.example".to_string(), "both,indexer".to_string()),
    ];
    crate::store::relay_config::save_relay_config(&dir.path, &relays).expect("save");
    let loaded = crate::store::relay_config::load_relay_config(&dir.path);
    assert_eq!(loaded, relays);
}

#[test]
fn cold_load_restores_inbox_triage_cache_through_set_data_dir() {
    use crate::inbox_llm::{TriageResult, TriageStatus};
    use std::collections::HashMap;

    let dir = TempDir::new("triage-cold-load");
    // Simulate a prior session having persisted triage scores to this dir.
    let mut persisted: HashMap<String, TriageResult> = HashMap::new();
    persisted.insert(
        "ep-1".to_string(),
        TriageResult::ready(
            0.91,
            "Highly relevant".to_string(),
            vec!["tech".to_string()],
            1_700_000_000,
        ),
    );
    persisted.insert("ep-2".to_string(), TriageResult::pending(1_700_000_500));
    crate::store::inbox_triage_cache::save_triage_cache(&dir.path, &persisted)
        .expect("seed triage cache");

    // Cold launch: a fresh handle with an empty cache binds to the dir.
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let handle = make_handle(store.clone(), rev.clone());
    let cache_arc = handle.inbox_triage_cache.clone();
    assert!(cache_arc.lock().unwrap().is_empty(), "cache starts empty");
    let ptr = Box::into_raw(handle);
    let cpath = CString::new(dir.path.to_str().unwrap()).unwrap();

    nmp_app_podcast_set_data_dir(ptr, cpath.as_ptr());

    // The FFI load block populated the handle's cache from disk...
    let restored = cache_arc.lock().unwrap();
    assert_eq!(restored.len(), 2, "both persisted entries restored");
    let ready = restored.get("ep-1").expect("ready entry restored");
    assert_eq!(ready.status, TriageStatus::Ready);
    assert!((ready.priority_score - 0.91).abs() < f32::EPSILON);
    assert_eq!(ready.priority_reason, "Highly relevant");
    assert_eq!(restored.get("ep-2").unwrap().status, TriageStatus::Pending);
    drop(restored);

    // ...and the restore bumped rev so the first snapshot poll surfaces it.
    assert_eq!(rev.load(Ordering::Relaxed), 1);

    let _ = unsafe { Box::from_raw(ptr) };
}

#[test]
fn cold_load_restores_agent_tasks_through_set_data_dir() {
    let dir = TempDir::new("tasks-cold-load");
    let persisted = vec![AgentTaskSummary {
        id: "task-1".to_owned(),
        title: "Remember".to_owned(),
        description: Some("from disk".to_owned()),
        intent_type: "remember_memory".to_owned(),
        intent_label: "Remember memory".to_owned(),
        intent_detail: Some("topic = rust".to_owned()),
        action_namespace: "podcast.memory".to_owned(),
        action_body: r#"{"op":"remember","key":"topic","value":"rust","source":"task"}"#.to_owned(),
        schedule: "daily".to_owned(),
        next_run_at: Some(1_700_000_000),
        last_run_at: Some(1_699_999_000),
        status: "completed".to_owned(),
        is_enabled: true,
    }];
    crate::store::agent_tasks::save_agent_tasks(&dir.path, &persisted).expect("seed agent tasks");

    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let handle = make_handle(store.clone(), rev.clone());
    // Step 6: tasks slot is now owned by state.tasks (TasksState).
    let tasks_slot = handle.state.tasks.tasks.share();
    let ptr = Box::into_raw(handle);
    let cpath = CString::new(dir.path.to_str().unwrap()).unwrap();

    nmp_app_podcast_set_data_dir(ptr, cpath.as_ptr());

    let restored = tasks_slot.lock().unwrap();
    assert_eq!(*restored, persisted);
    assert_eq!(restored[0].action_namespace, "podcast.memory");
    drop(restored);
    assert_eq!(rev.load(Ordering::Relaxed), 1);

    let _ = unsafe { Box::from_raw(ptr) };
}

#[test]
fn cold_load_empty_agent_tasks_overrides_seed() {
    let dir = TempDir::new("tasks-empty-load");
    crate::store::agent_tasks::save_agent_tasks(&dir.path, &[]).expect("seed empty tasks");

    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let handle = make_handle(store.clone(), rev.clone());
    // Step 6: tasks slot is now owned by state.tasks (TasksState).
    let tasks_slot = handle.state.tasks.tasks.share();
    *tasks_slot.lock().unwrap() = crate::tasks_handler::default_seed();
    let ptr = Box::into_raw(handle);
    let cpath = CString::new(dir.path.to_str().unwrap()).unwrap();

    nmp_app_podcast_set_data_dir(ptr, cpath.as_ptr());

    assert!(tasks_slot.lock().unwrap().is_empty());
    assert_eq!(rev.load(Ordering::Relaxed), 1);

    let _ = unsafe { Box::from_raw(ptr) };
}

#[test]
fn loading_existing_library_bumps_rev_so_ios_re_polls() {
    let dir = TempDir::new("reload");
    // Pre-populate the directory with one podcast.
    {
        let mut warm = PodcastStore::new();
        warm.set_data_dir(dir.path.clone());
        warm.subscribe(podcast_core::Podcast::new("Pre-loaded"), vec![]);
    }
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let handle = make_handle(store.clone(), rev.clone());
    let ptr = Box::into_raw(handle);
    let cpath = CString::new(dir.path.to_str().unwrap()).unwrap();
    nmp_app_podcast_set_data_dir(ptr, cpath.as_ptr());
    assert_eq!(store.lock().unwrap().podcast_count(), 1);
    assert_eq!(rev.load(Ordering::Relaxed), 1);
    let _ = unsafe { Box::from_raw(ptr) };
}
