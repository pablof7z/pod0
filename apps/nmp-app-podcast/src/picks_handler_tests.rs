use super::*;
use chrono::{TimeZone, Utc};
use podcast_core::{Episode, Podcast, PodcastId};
use url::Url;
use uuid::Uuid;
fn make_podcast(title: &str) -> Podcast {
    Podcast::new(title)
}
fn make_episode(podcast_id: PodcastId, title: &str, ts: i64) -> Episode {
    Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        format!("guid-{}", Uuid::new_v4()),
        title,
        Url::parse("https://example.com/audio.mp3").unwrap(),
        Utc.timestamp_opt(ts, 0).single().unwrap(),
    )
}
#[test]
fn collect_candidates_returns_all_episodes() {
    let mut store = PodcastStore::new();
    let p1 = make_podcast("Show A");
    let p1_id = p1.id;
    let p2 = make_podcast("Show B");
    let p2_id = p2.id;
    store.subscribe(
        p1,
        vec![
            make_episode(p1_id, "A-1", 100),
            make_episode(p1_id, "A-2", 200),
        ],
    );
    store.subscribe(p2, vec![make_episode(p2_id, "B-1", 300)]);
    let cands = collect_candidates(&store);
    assert_eq!(cands.len(), 3);
    // Show titles come through.
    let titles: std::collections::HashSet<&str> =
        cands.iter().map(|c| c.podcast_title.as_str()).collect();
    assert!(titles.contains("Show A"));
    assert!(titles.contains("Show B"));
}
#[test]
fn refresh_picks_writes_into_slot_and_bumps_rev() {
    let mut s = PodcastStore::new();
    let p = make_podcast("Refresh Show");
    let pid = p.id;
    s.subscribe(p, vec![make_episode(pid, "ep-1", 100)]);
    let store = Arc::new(Mutex::new(s));
    let slot = Arc::new(Mutex::new(Vec::<AgentPickSummary>::new()));
    let rev = Arc::new(AtomicU64::new(0));
    refresh_picks_into_slot(&store, &slot, &rev);
    let written = slot.lock().unwrap();
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].podcast_title, "Refresh Show");
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}
#[test]
fn handle_refresh_returns_ok_envelope_and_populates_slot() {
    let mut s = PodcastStore::new();
    let p = make_podcast("Envelope Show");
    let pid = p.id;
    s.subscribe(p, vec![make_episode(pid, "ep-1", 100)]);
    let store = Arc::new(Mutex::new(s));
    let slot = Arc::new(Mutex::new(Vec::<AgentPickSummary>::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
    let in_progress = Arc::new(AtomicBool::new(false));

    // The immediate heuristic stamp runs synchronously inside handle_refresh,
    // so the slot is populated before the background scoring task (which would
    // need a live Ollama) even starts.
    let resp = handle_refresh_inner(&store, &slot, &rev, &runtime, &in_progress, None);
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["status"], "scoring_started");
    assert_eq!(slot.lock().unwrap().len(), 1);
    // The first stamp came from the synchronous heuristic path.
    assert_eq!(slot.lock().unwrap()[0].podcast_title, "Envelope Show");
}

#[test]
fn handle_refresh_second_call_while_in_progress_is_guarded() {
    // Pre-set the in-progress flag to simulate a scoring pass already running.
    // The heuristic still re-stamps, but no second background pass is spawned.
    let mut s = PodcastStore::new();
    let p = make_podcast("Guard Show");
    let pid = p.id;
    s.subscribe(p, vec![make_episode(pid, "ep-1", 100)]);
    let store = Arc::new(Mutex::new(s));
    let slot = Arc::new(Mutex::new(Vec::<AgentPickSummary>::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
    let in_progress = Arc::new(AtomicBool::new(true)); // already running

    let resp = handle_refresh_inner(&store, &slot, &rev, &runtime, &in_progress, None);
    assert_eq!(resp["status"], "already_running");
    // Heuristic stamp still happened.
    assert_eq!(slot.lock().unwrap().len(), 1);
}

#[test]
fn compute_picks_scored_overrides_heuristic_order() {
    use crate::ffi::actions::picks_module::compute_picks_scored;
    use std::collections::HashMap;
    // Newest-first heuristic would rank the newer episode first; an LLM score
    // flips that — the older episode with the higher score wins.
    let candidates = vec![
        CandidateEpisode {
            episode_id: "old-high".into(),
            episode_title: "Deep dive".into(),
            podcast_id: "pod-1".into(),
            podcast_title: "Show A".into(),
            artwork_url: None,
            published_at: 100,
            duration_secs: None,
        },
        CandidateEpisode {
            episode_id: "new-low".into(),
            episode_title: "Filler".into(),
            podcast_id: "pod-2".into(),
            podcast_title: "Show B".into(),
            artwork_url: None,
            published_at: 9_000,
            duration_secs: None,
        },
    ];
    let mut scores: HashMap<String, (f32, String)> = HashMap::new();
    scores.insert("old-high".into(), (0.95, "Must-listen analysis.".into()));
    scores.insert("new-low".into(), (0.20, "Skippable.".into()));

    let picks = compute_picks_scored(candidates, &scores);
    assert_eq!(picks.len(), 2);
    // Higher LLM score wins despite being older.
    assert_eq!(picks[0].episode_id, "old-high");
    assert_eq!(picks[0].pick_reason, "Must-listen analysis.");
    assert!(picks[0].pick_score > picks[1].pick_score);
}
#[test]
fn refresh_picks_on_empty_store_yields_empty_slot() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let slot = Arc::new(Mutex::new(Vec::<AgentPickSummary>::new()));
    let rev = Arc::new(AtomicU64::new(0));
    refresh_picks_into_slot(&store, &slot, &rev);
    assert!(slot.lock().unwrap().is_empty());
    // Slot rev still bumps so the next snapshot frame observes the slot.
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}

#[test]
fn listening_profile_empty_on_cold_start() {
    // No played / in-progress / starred episodes anywhere → empty profile so
    // the prompt degrades to general-interest scoring.
    let mut s = PodcastStore::new();
    let p = make_podcast("Fresh Show");
    let pid = p.id;
    s.subscribe(p, vec![make_episode(pid, "ep-1", 100)]);
    assert!(build_listening_profile(&s).is_empty());
}

#[test]
fn listening_profile_surfaces_most_engaged_shows_first() {
    let mut s = PodcastStore::new();

    // Show A: 2 finished listens (weight 4) — strongest engagement.
    let pa = make_podcast("Show A");
    let pa_id = pa.id;
    let mut a1 = make_episode(pa_id, "A-1", 100);
    a1.played = true;
    let mut a2 = make_episode(pa_id, "A-2", 200);
    a2.played = true;
    s.subscribe(pa, vec![a1, a2]);

    // Show B: 1 in-progress (weight 1) — weaker.
    let pb = make_podcast("Show B");
    let pb_id = pb.id;
    let mut b1 = make_episode(pb_id, "B-1", 300);
    b1.position_secs = 120.0;
    s.subscribe(pb, vec![b1]);

    // Show C: no engagement → excluded entirely.
    let pc = make_podcast("Show C");
    let pc_id = pc.id;
    s.subscribe(pc, vec![make_episode(pc_id, "C-1", 400)]);

    let profile = build_listening_profile(&s);
    let a_pos = profile.find("Show A").expect("Show A present");
    let b_pos = profile.find("Show B").expect("Show B present");
    assert!(a_pos < b_pos, "more-engaged show must rank first");
    assert!(!profile.contains("Show C"), "un-engaged show excluded");
    assert!(profile.contains("played 2"));
    assert!(profile.contains("1 in progress"));
}

#[test]
fn listening_profile_counts_starred_episodes() {
    let mut s = PodcastStore::new();
    let p = make_podcast("Starred Show");
    let pid = p.id;
    let mut ep = make_episode(pid, "fav", 100);
    ep.is_starred = true;
    s.subscribe(p, vec![ep]);

    let profile = build_listening_profile(&s);
    assert!(profile.contains("Starred Show"));
    assert!(profile.contains("1 starred"));
}
