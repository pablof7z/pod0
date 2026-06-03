use super::*;
use crate::download::DownloadQueue;
use crate::ffi::handle::PodcastHandle;
use crate::ffi::projections::{AgentPickSummary, NostrShowSummary, PodcastSummary, VoiceState};
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::store::identity::IdentityStore;
use crate::store::{PodcastKeyStore, PodcastStore};
use std::collections::HashSet;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
/// Build a `PodcastHandle` with a NULL `app` pointer — these tests only
/// exercise the data-dir path, which never touches `app`.
fn make_handle(store: Arc<Mutex<PodcastStore>>, rev: Arc<AtomicU64>) -> Box<PodcastHandle> {
    use std::collections::HashMap;
    Box::new(PodcastHandle {
        app: std::ptr::null_mut(),
        player_actor: Arc::new(Mutex::new(PlayerActor::new())),
        store: store.clone(),
        identity: Arc::new(Mutex::new(IdentityStore::new())),
        rev: rev.clone(),
        search_results: Arc::new(Mutex::new(Vec::<PodcastSummary>::new())),
        nostr_results: Arc::new(Mutex::new(Vec::<NostrShowSummary>::new())),
        snapshot_cache: Arc::new(Mutex::new(None)),
        queue: Arc::new(Mutex::new(PlaybackQueue::new())),
        download_queue: Arc::new(Mutex::new(DownloadQueue::new())),
        wiki_articles: Arc::new(Mutex::new(Vec::new())),
        wiki_search_results: Arc::new(Mutex::new(Vec::new())),
        picks: Arc::new(Mutex::new(Vec::<AgentPickSummary>::new())),
        agent_tasks: Arc::new(Mutex::new(Vec::new())),
        knowledge_search_results: Arc::new(Mutex::new(Vec::new())),
        knowledge_store: Arc::new(Mutex::new(podcast_knowledge::KnowledgeStore::new())),
        clips: Arc::new(Mutex::new(Vec::new())),
        transcripts: Arc::new(Mutex::new(HashMap::new())),
        dismissed_episode_ids: Arc::new(Mutex::new(HashSet::new())),
        podcast_keys: Arc::new(Mutex::new(PodcastKeyStore::new())),
        publish_state: Arc::new(Mutex::new(HashMap::new())),
        voice_state: Arc::new(Mutex::new(VoiceState::default())),
        voice_conversation: crate::voice_conversation::VoiceConversationManager::new(
            std::ptr::null_mut(),
            Arc::new(Mutex::new(Vec::new())),
            store.clone(),
            Arc::new(Mutex::new(VoiceState::default())),
            Arc::new(tokio::runtime::Runtime::new().unwrap()),
            rev.clone(),
        ),
        conversation: Arc::new(Mutex::new(Vec::new())),
        agent_busy: Arc::new(AtomicBool::new(false)),
        agent_touched: Arc::new(AtomicBool::new(false)),
        categories: Arc::new(Mutex::new(HashMap::new())),
        inbox_triage_cache: Arc::new(Mutex::new(HashMap::new())),
        inbox_triage_in_progress: Arc::new(AtomicBool::new(false)),
        comments_cache: Arc::new(Mutex::new(HashMap::new())),
        social: Arc::new(Mutex::new(None)),
        agent_notes: Arc::new(Mutex::new(Vec::new())),
        runtime: Arc::new(tokio::runtime::Runtime::new().unwrap()),
    })
}
struct TempDir { path: PathBuf }
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
impl Drop for TempDir { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.path); } }
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
        TriageResult::ready(0.91, "Highly relevant".to_string(), vec!["tech".to_string()], 1_700_000_000),
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
