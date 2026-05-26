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
        store,
        identity: Arc::new(Mutex::new(IdentityStore::new())),
        rev,
        search_results: Arc::new(Mutex::new(Vec::<PodcastSummary>::new())),
        nostr_results: Arc::new(Mutex::new(Vec::<NostrShowSummary>::new())),
        snapshot_cache: Arc::new(Mutex::new(None)),
        briefing: Arc::new(Mutex::new(None)),
        queue: Arc::new(Mutex::new(PlaybackQueue::new())),
        download_queue: Arc::new(Mutex::new(DownloadQueue::new())),
        wiki_articles: Arc::new(Mutex::new(Vec::new())),
        wiki_search_results: Arc::new(Mutex::new(Vec::new())),
        picks: Arc::new(Mutex::new(Vec::<AgentPickSummary>::new())),
        agent_tasks: Arc::new(Mutex::new(Vec::new())),
        knowledge_search_results: Arc::new(Mutex::new(Vec::new())),
        tts_episodes: Arc::new(Mutex::new(Vec::new())),
        clips: Arc::new(Mutex::new(Vec::new())),
        transcripts: Arc::new(Mutex::new(HashMap::new())),
        dismissed_episode_ids: Arc::new(Mutex::new(HashSet::new())),
        podcast_keys: Arc::new(Mutex::new(PodcastKeyStore::new())),
        publish_state: Arc::new(Mutex::new(HashMap::new())),
        voice_state: Arc::new(Mutex::new(VoiceState::default())),
        conversation: Arc::new(Mutex::new(Vec::new())),
        agent_busy: Arc::new(AtomicBool::new(false)),
        agent_touched: Arc::new(AtomicBool::new(false)),
        categories: Arc::new(Mutex::new(HashMap::new())),
        inbox_triage_cache: Arc::new(Mutex::new(HashMap::new())),
        comments_cache: Arc::new(Mutex::new(HashMap::new())),
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
