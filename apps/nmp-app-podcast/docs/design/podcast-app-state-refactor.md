# Design Doc — Eliminating the Twin God-Roots (`PodcastAppState`)

Status: PROPOSED · Scope: `apps/nmp-app-podcast/src` · Author: architecture review Point 2 follow-up
Audience: implementing engineer (multi-PR, strangler) · Last reviewed against `main` @ commit `94cc1326`

---

## 0. Problem statement (the defect, precisely)

Two structs hold the *same* state inventory, mirrored field-for-field, differing only in
read-vs-write role:

- `src/ffi/handle.rs::PodcastHandle` — the **reader seam** (~34 `Arc<…>` fields). `build_podcast_update` locks each slot and projects it into `PodcastUpdate`.
- `src/host_op_handler.rs::PodcastHostOpHandler` — the **writer seam** (~36 fields). Each `handle_*_action` mutates a slot and bumps `rev`.

`src/ffi/register.rs` clones ~30 `Arc<Mutex<…>>` **twice**: once into the
31-argument `PodcastHostOpHandler::new(...)` (`#[allow(clippy::too_many_arguments)]`) and again
into the `PodcastHandle { … }` literal. Adding one feature is 6-file shotgun surgery:
`handle.rs` + `host_op_handler.rs` (field + constructor arg + constructor body) + `register.rs`
(the `Arc::new` + two clone sites) + `snapshot.rs` (read) + the feature's `*_handler.rs`.

The state is **not a domain model** — it is a flat bag of per-feature caches with three
*undocumented-in-type* durability classes intermixed:

- **Persisted** (lives in `PodcastStore` → `podcasts.json`): library, positions, ~80 settings, memory_facts, triage_state, ad_segments, podcast_keys (separate file), inbox_triage_cache (separate file), episode_events (per-episode files).
- **Session-only** (evaporates on restart): `clips`, `transcripts` (handle-side cache), `dismissed_episode_ids`, `comments_cache`, `agent_notes`, `social`, `conversation`, `voice_state`, `search_results`, `nostr_results`, `wiki_search_results`, `knowledge_search_results`, `publish_state`.
- **Derived/recomputed** (rebuilt from persisted state on demand): `picks`, `categories`, `wiki_articles` (currently session-only but logically derived), `knowledge_store` (RAG index, re-indexable), `clean_html_cache`, `snapshot_cache`.

The durability class is **invisible at the type level** — you learn it only by reading a doc
comment, which is exactly how session-only slots silently become "should this persist?" bugs.

### Root cause

There is no composed state type. The "state" is the *union of two constructors*. Because the
two constructors are hand-maintained mirrors, the only thing keeping them in sync is discipline,
and the only home for a feature's logic is a free function that re-receives 4–6 `Arc`s on every
call (`handle_knowledge_action(action, &store, &slot, &knowledge_store, &rev)`).

---

## 1. North-star (what we build toward)

One composed `PodcastAppState` tree, owned behind a single `Arc`, holding per-feature
**substates**. Each substate is a small struct with its own slots and an **explicit, type-level
`Durability` marker**. `PodcastHandle` and `PodcastHostOpHandler` each hold *one* field — the
`Arc<PodcastAppState>` — plus the genuinely role-specific bits (the reader's `snapshot_cache` /
`clean_html_cache`; nothing role-specific on the writer once handlers are methods).

The **handle/handler split is PRESERVED** — it is a principled reader-seam / writer-seam
boundary, and the FFI requires the two to be separately `unsafe impl Send` over the `*mut NmpApp`
pointer. What we kill is the duplicated *inventory*, not the split.

Each `handle_*_action` free function becomes a **method on its owning substate** (the missing
service layer). The host-op router calls `self.state.knowledge.handle(action)` instead of
`handle_knowledge_action(action, &self.store, &self.knowledge_search_results, …)`.

---

## 2. State inventory (paired mirrors + writers/readers + durability)

Legend — **W**riter thread: `A`=actor thread (host-op dispatch), `R`=report FFI thread
(audio/download/http — off-actor), `T`=tokio task (LLM/relay, off-actor), `O`=kernel event
observer (relay events, off-actor on the NMP pool). **Durability**: `P`=persisted,
`S`=session, `D`=derived.

| Slot (mirror name on both structs) | Writers | Readers | Dur | Feature / substate |
|---|---|---|---|---|
| `store: Arc<Mutex<PodcastStore>>` | A, R, T, O | A(snapshot), R | **P** | **Library** (canonical persisted root — see §3.4) |
| `identity: Arc<Mutex<IdentityStore>>` | A | A(snapshot), O(agent_notes) | **P** | **Identity** |
| `player_actor: Arc<Mutex<PlayerActor>>` | A, R(audio) | A(snapshot), R | **S** (positions persist via store) | **Playback** |
| `queue: Arc<Mutex<PlaybackQueue>>` | A, R(audio auto-advance) | A(snapshot), R | **P** (mirrored into store.cached_queue) | **Playback** |
| `download_queue: Arc<Mutex<DownloadQueue>>` | A, R(download) | A(snapshot), R | **S** | **Downloads** |
| `search_results: Arc<Mutex<Vec<PodcastSummary>>>` | A | A(snapshot) | **S** | **Discovery** |
| `nostr_results: Arc<Mutex<Vec<NostrShowSummary>>>` | O(discovery) | A(snapshot) | **S** | **Discovery** |
| `wiki_articles: Arc<Mutex<Vec<WikiArticle>>>` | A, T | A(snapshot) | **S** (logically D) | **Wiki** |
| `wiki_search_results: Arc<Mutex<Vec<WikiArticle>>>` | A | A(snapshot) | **S** | **Wiki** |
| `picks: Arc<Mutex<Vec<AgentPickSummary>>>` | A, T(LLM) | A(snapshot) | **D** | **Picks** |
| `picks_score_in_progress: Arc<AtomicBool>` (writer-only) | A, T | T | **D** | **Picks** |
| `agent_tasks: Arc<Mutex<Vec<AgentTaskSummary>>>` | A | A(snapshot) | **P** (store.agent_tasks) | **Tasks** |
| `knowledge_search_results: Arc<Mutex<Vec<KnowledgeSearchResult>>>` | A | A(snapshot) | **S** | **Knowledge** |
| `knowledge_store: Arc<Mutex<KnowledgeStore>>` | A | A | **D** (re-indexable) | **Knowledge** |
| `clips: Arc<Mutex<Vec<ClipRecord>>>` | A | A(snapshot) | **S** | **Clips** |
| `transcripts: Arc<Mutex<HashMap<String,Vec<TranscriptEntry>>>>` | A | A(snapshot) | **S** | **Transcripts** |
| `dismissed_episode_ids: Arc<Mutex<HashSet<String>>>` | A | A(snapshot) | **S** | **Inbox** |
| `inbox_triage_cache: Arc<Mutex<HashMap<String,TriageResult>>>` | A, T | A(snapshot), T | **P** (store/inbox_triage_cache file) | **Inbox** |
| `inbox_triage_in_progress: Arc<AtomicBool>` | A, T | A(snapshot), T | **S** | **Inbox** |
| `podcast_keys: Arc<Mutex<PodcastKeyStore>>` | A | A(snapshot) | **P** (own file) | **Publish** |
| `publish_state: Arc<Mutex<HashMap<String,OwnedPublishState>>>` | A | A(snapshot) | **S** | **Publish** |
| `voice_state: Arc<Mutex<VoiceState>>` | A, R(voice) | A(snapshot) | **S** | **Voice** |
| `categories: Arc<Mutex<HashMap<String,Vec<String>>>>` | A, T(LLM), O(feed_fetch) | A(snapshot) | **D** | **Categories** |
| `categorization_in_progress: Arc<AtomicBool>` (writer-only) | A, T | T | **D** | **Categories** |
| `comments_cache: Arc<Mutex<HashMap<String,Vec<CommentSummary>>>>` | A, O(comments) | A(snapshot) | **S** | **Comments** |
| `viewed_comments_episode_id: Arc<Mutex<Option<String>>>` | A | A(snapshot) | **S** | **Comments** |
| `social: Arc<Mutex<Option<SocialSnapshot>>>` | A(via T) | A(snapshot) | **S** | **Social** |
| `agent_notes: Arc<Mutex<Vec<AgentNoteSummary>>>` | O(agent_notes) | A(snapshot) | **S** | **AgentNotes** |
| `conversation: Arc<Mutex<Vec<AgentMessageSummary>>>` (handle) / inside `agent_chat` (handler) | A, T | A(snapshot) | **S** | **AgentChat** |
| `agent_busy: Arc<AtomicBool>` | A, T | A(snapshot) | **S** | **AgentChat** |
| `agent_touched: Arc<AtomicBool>` | A | A(snapshot) | **S** | **AgentChat** |
| `feedback: nmp_feedback::FeedbackRuntime` (already self-composed `Arc`) | O(feedback) | A(snapshot) | **S** (cache) | **Feedback** |
| `feed_fetch: Arc<FeedFetchCoordinator>` (already self-composed `Arc`) | A, R(http) | — | **S** | **Downloads/Discovery infra** |
| `runtime: Arc<Runtime>` | — | A, T | n/a (infra) | **shared infra** |
| `rev: Arc<AtomicU64>` | all | all | n/a (infra) | **shared infra** |
| `snapshot_signal: Option<SnapshotUpdateSignal>` | — | all | n/a (infra) | **shared infra** |
| `app: *mut NmpApp` | — | A, R | n/a (infra, NOT in shared state) | **role-specific** |
| `snapshot_cache: Arc<Mutex<Option<(u64,String)>>>` (handle-only) | A(snapshot) | A(snapshot) | **D** | **reader-only** |
| `clean_html_cache: Arc<Mutex<HashMap<u64,String>>>` (handle-only) | A(snapshot) | A(snapshot) | **D** | **reader-only** |
| `voice_conversation: VoiceConversationManager` (handle-only, holds own Arcs) | R(voice) | — | **S** | **Voice** |

### Key observations driving the design

1. **Almost everything is actor-thread-only.** The *only* slots written off the actor thread are:
   - `store`, `player_actor`, `queue`, `download_queue` — from the **report FFI threads** (`audio_report`, `download_report`, `http_report`).
   - `picks`, `categories`, `wiki_articles`, `inbox_triage_cache`, `conversation`, `social` — from **tokio LLM/relay tasks** (writer side spawns; these write back under their own lock).
   - `nostr_results`, `comments_cache`, `agent_notes`, `feedback`, `categories` — from **kernel event observers** (off-actor on NMP's pool).

   This is the load-bearing fact for §6 lock granularity: a single mega-lock would serialize the
   snapshot read against every report-thread and tokio writer — a measured regression. Fine-grained
   per-substate locks are *buying real concurrency* and must be preserved.

2. **The `*_in_progress` atomics are writer-only re-entrancy guards** never read by the snapshot
   except `inbox_triage_in_progress`. They belong **inside** their substate, not mirrored.

3. **`feedback` and `feed_fetch` are already self-composed `Arc` newtypes** — they are the *proof of
   concept* for the target shape. The refactor generalizes them.

4. **Three slots are persisted but live OUTSIDE `PodcastStore`**: `podcast_keys` (own file),
   `inbox_triage_cache` (own file), and `agent_tasks` (inside store). This matters for §3.4 —
   `PodcastStore` is *not* the only persisted root; the durability marker must be per-slot.

---

## 3. The `PodcastAppState` tree

### 3.1 Top-level composition

```rust
// src/state/mod.rs  (NEW)

/// The single composed root owning every per-feature substate. One `Arc` of
/// this is shared by the reader seam (`PodcastHandle`) and the writer seam
/// (`PodcastHostOpHandler`). Replaces the field-for-field mirror and the
/// 31-arg constructor.
pub struct PodcastAppState {
    // ---- canonical persisted root (see §3.4) ----
    pub library: LibraryState,        // wraps Arc<Mutex<PodcastStore>> + identity

    // ---- playback domain ----
    pub playback: PlaybackState,      // player_actor + queue + download_queue

    // ---- discovery / social ----
    pub discovery: DiscoveryState,    // search_results + nostr_results
    pub social: SocialState,          // social + agent_notes
    pub comments: CommentsState,      // comments_cache + viewed_comments_episode_id

    // ---- AI features ----
    pub wiki: WikiState,
    pub picks: PicksState,
    pub categories: CategoriesState,
    pub knowledge: KnowledgeState,
    pub inbox: InboxState,
    pub agent_chat: AgentChatState,
    pub voice: VoiceState_,           // renamed to avoid clash with projection VoiceState
    pub transcripts: TranscriptsState,
    pub clips: ClipsState,
    pub tasks: TasksState,

    // ---- publishing ----
    pub publish: PublishState,

    // ---- already-composed runtimes (just move them in) ----
    pub feedback: nmp_feedback::FeedbackRuntime,
    pub feed_fetch: Arc<crate::feed_fetch::FeedFetchCoordinator>,

    // ---- shared infra (not a feature; injected into every substate) ----
    pub infra: Infra,
}

/// Cross-cutting infrastructure every substate needs to bump the snapshot and
/// spawn off-actor work. Cloned into substates at construction so a substate
/// method needs no extra params to bump rev.
#[derive(Clone)]
pub struct Infra {
    pub rev: Arc<AtomicU64>,
    pub signal: Option<SnapshotUpdateSignal>,
    pub runtime: Arc<Runtime>,
}

impl Infra {
    /// The single rev-bump discipline, lifted out of every handler. Bumps the
    /// atomic and (when wired) posts MarkChangedSinceEmit. Replaces the
    /// open-coded `match self.snapshot_signal { Some(s)=>s.bump(), None=>rev.fetch_add }`
    /// repeated in ~12 handlers.
    pub fn bump(&self) {
        match &self.signal {
            Some(s) => s.bump(),
            None => { self.rev.fetch_add(1, Ordering::Relaxed); }
        }
    }
    pub fn bump_if(&self, changed: bool) { if changed { self.bump(); } }
}
```

### 3.2 Durability marker (type-level, not a doc comment)

Durability becomes a **type-level tag** so a session-only slot cannot silently become persisted.
The marker is a zero-cost `PhantomData` tag on a `Slot<T, D>` wrapper that wraps the existing
`Arc<Mutex<T>>`. It does not change locking — it documents and *enforces* (via the persistence
trait bound) the durability class.

```rust
// src/state/slot.rs  (NEW)

pub trait Durability {}
pub struct Persisted;  impl Durability for Persisted {}
pub struct Session;    impl Durability for Session {}
pub struct Derived;    impl Durability for Derived {}

/// A single shared state slot. `D` is its durability class — a compile-time
/// fact, not a comment. Only `Slot<_, Persisted>` exposes `persist()`; a
/// `Session` slot literally cannot call it (no such method), so the
/// "accidentally persisted a session slot" bug is unrepresentable.
pub struct Slot<T, D: Durability> {
    inner: Arc<Mutex<T>>,
    _dur: PhantomData<D>,
}

impl<T, D: Durability> Slot<T, D> {
    pub fn new(value: T) -> Self { Self { inner: Arc::new(Mutex::new(value)), _dur: PhantomData } }
    /// The ONE lock accessor. `read`/`write` are the same lock; named for intent.
    pub fn lock(&self) -> std::sync::LockResult<MutexGuard<'_, T>> { self.inner.lock() }
    /// Clone the Arc for an off-actor writer (report thread / tokio task / observer).
    pub fn share(&self) -> Arc<Mutex<T>> { self.inner.clone() }
}

// Persistence is ONLY available on Persisted slots — enforced by the impl bound.
impl<T: PersistTo> Slot<T, Persisted> {
    pub fn persist(&self) -> Result<(), PersistError> { /* delegate to T */ }
}
```

> Atomics (`AtomicBool` guards, `rev`) stay bare `Arc<AtomicU64/Bool>` — `Slot` is for
> `Mutex`-guarded slots. The marker still applies conceptually (all atomics are `Derived`/`Session`).

For slots whose persistence is owned by `PodcastStore` (positions, queue, tasks, triage cache,
memory_facts) the substate holds a `Slot<_, Session>` *projection cache* and the **`Persisted`**
classification lives on the `PodcastStore` field — i.e. the durability marker sits wherever the
write-through actually happens. This keeps the marker honest: the cache slot is genuinely Session
(it's a derived view), and the store field is genuinely Persisted.

### 3.3 Representative substate structs

```rust
// src/state/knowledge.rs  (NEW)  — the pattern-setter (low-risk, §5)
pub struct KnowledgeState {
    /// Transient RAG search results projected into PodcastUpdate.knowledge_search_results.
    pub results: Slot<Vec<KnowledgeSearchResult>, Session>,
    /// In-memory RAG chunk index — re-indexable from persisted transcripts.
    pub index: Slot<KnowledgeStore, Derived>,
    infra: Infra,
    /// The canonical library (read-only borrow for transcript text on index).
    store: Arc<Mutex<PodcastStore>>,
}

// src/state/wiki.rs
pub struct WikiState {
    pub articles: Slot<Vec<WikiArticle>, Session>,
    pub search_results: Slot<Vec<WikiArticle>, Session>,
    infra: Infra,
    store: Arc<Mutex<PodcastStore>>,
    knowledge_index: Arc<Mutex<KnowledgeStore>>, // shared with KnowledgeState (RAG context)
}

// src/state/inbox.rs
pub struct InboxState {
    pub dismissed: Slot<HashSet<String>, Session>,
    pub triage_cache: Slot<HashMap<String, TriageResult>, Session>, // store owns the file persist
    pub triage_in_progress: Arc<AtomicBool>,
    infra: Infra,
    store: Arc<Mutex<PodcastStore>>,
}

// src/state/picks.rs
pub struct PicksState {
    pub picks: Slot<Vec<AgentPickSummary>, Derived>,
    pub score_in_progress: Arc<AtomicBool>,
    infra: Infra,
    store: Arc<Mutex<PodcastStore>>,
}

// src/state/playback.rs  — the cross-thread one (report FFI writes here)
pub struct PlaybackState {
    pub player: Slot<PlayerActor, Session>,
    pub queue: Slot<PlaybackQueue, Persisted>,        // write-through to store.cached_queue
    pub downloads: Slot<DownloadQueue, Session>,
    infra: Infra,
}
```

`LibraryState` wraps the canonical persisted root:

```rust
// src/state/library.rs
pub struct LibraryState {
    pub store: Arc<Mutex<PodcastStore>>,  // canonical persisted root (§3.4)
    pub identity: Slot<IdentityStore, Persisted>,
    infra: Infra,
}
```

### 3.4 How `PodcastStore` relates

**`PodcastStore` STAYS the canonical persisted root.** It is *not* dissolved into substates — that
would be a second, larger refactor (it owns the cross-language `podcasts.json` fixture contract and
~80 settings with a Swift mirror, per `store/mod.rs` doc). Splitting it is **out of scope** and
called out as future work (§7).

Instead, `PodcastStore` becomes **one shared dependency** that substates borrow:

- `LibraryState` owns the `Arc<Mutex<PodcastStore>>`.
- Substates that read/write persisted data (Knowledge reads transcripts, Inbox reads episodes,
  Picks/Categories read the library, Playback writes positions) hold a **clone of that same `Arc`**.
- The substate's *own* slots are the **transient layer around** the persisted store: `KnowledgeState.results`
  is the session projection; `PodcastStore.transcripts` is the persisted source.

This makes the durability boundary crisp: **`PodcastStore` = the Persisted island; substate `Slot`s =
the Session/Derived layer that wraps it.** The three out-of-store persisted slots (`podcast_keys`,
`inbox_triage_cache`, `agent_tasks`) keep their own persistence but are now reachable as
`Slot<_, Persisted>` (keys) or via the store (tasks/triage), so the marker stays accurate.

---

## 4. Re-homed handlers (the service layer)

Free functions taking N `Arc`s become methods on the owning substate. The substate already holds
`infra` (rev + signal + runtime) and its dependency `Arc`s, so the call site collapses.

### 4.1 Knowledge (the pattern-setter)

Before (`host_op_handler.rs::handle`):
```rust
if let Ok(a) = serde_json::from_str::<KnowledgeAction>(action_json) {
    return crate::knowledge::handle_knowledge_action(
        a, &self.store, &self.knowledge_search_results, &self.knowledge_store, &self.rev,
    );
}
```

After (post-Point-1 router):
```rust
// in handle(), under `match ns { "podcast.knowledge" => ... }`
"podcast.knowledge" => self.state.knowledge.handle(parse::<KnowledgeAction>(env)?),
```

```rust
// src/state/knowledge.rs
impl KnowledgeState {
    pub fn handle(&self, action: KnowledgeAction) -> serde_json::Value {
        match action {
            KnowledgeAction::Search { query }      => self.search(query),
            KnowledgeAction::ClearResults          => self.clear_results(),
            KnowledgeAction::IndexEpisode { episode_id } => self.index_episode(episode_id),
        }
    }

    fn index_episode(&self, episode_id: String) -> serde_json::Value {
        let text = match self.store.lock() {
            Ok(s) => match s.transcript_for(&episode_id) {
                Some(t) => t.to_owned(),
                None => return serde_json::json!({"ok": true, "status": "no_transcript"}),
            },
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        let chunks = chunk_transcript_text(&episode_id, &text);
        let chunk_count = chunks.len();
        match self.index.lock() {
            Ok(mut ks) => { ks.delete_episode(&episode_id);
                            for c in chunks { ks.upsert(KnowledgeChunk::without_embedding(c)); } }
            Err(_) => return serde_json::json!({"ok": false, "error": "knowledge_store poisoned"}),
        }
        self.infra.bump();   // <-- single rev-bump discipline, was rev.fetch_add(1, Relaxed)
        serde_json::json!({"ok": true, "status": "indexed", "chunk_count": chunk_count})
    }
    // search(), clear_results() identical bodies, `self.infra.bump()` for the rev discipline.
}
```

The existing pure helpers (`chunk_transcript_text`, the BM25 ranker) stay free functions in
`knowledge.rs` — only the *Arc-threading shell* moves onto the method. The unit tests that build a
`PodcastStore` + `KnowledgeStore` directly keep working by constructing a `KnowledgeState` via a
test constructor (`KnowledgeState::for_test(store, infra)`).

### 4.2 Wiki

```rust
impl WikiState {
    pub fn handle(&self, action: WikiAction) -> serde_json::Value {
        // body of handle_wiki_action_inner, with:
        //   self.articles.lock(), self.search_results.lock(), self.store, self.knowledge_index
        //   self.infra.runtime for runtime.block_on(synthesize_summary), self.infra.bump()
        // The `_with_signal` / non-signal fork DISAPPEARS — infra.bump() handles both.
    }
}
```

This removes the entire `handle_wiki_action` / `handle_wiki_action_with_signal` pair (and the
matching pairs in inbox, picks, categorization, chapters): the signal-vs-no-signal duplication was
*only* there because the free functions couldn't reach a unified bump. `infra.bump()` unifies it.

### 4.3 Inbox + auto-triggers

```rust
impl InboxState {
    pub fn handle(&self, action: InboxAction) -> serde_json::Value { /* dismiss/triage/mark */ }

    /// Pure projection (was free `build_inbox`). Reads store + dismissed + triage_cache.
    pub fn project(&self) -> Vec<InboxItem> { build_inbox(&self.store, &self.dismissed.share(), &self.triage_cache.share()) }

    /// Proactive trigger called from the snapshot path (§4.4 note on reader access).
    pub fn maybe_enqueue_triage(&self) { /* runtime.spawn guarded by triage_in_progress */ }
}
```

The two auto-trigger methods currently on `PodcastHostOpHandler` (`auto_categorize`,
`auto_refresh_picks`, called from feed-refresh) become `self.state.categories.auto_run()` and
`self.state.picks.auto_refresh()` — same bodies, `self.infra` supplies runtime + signal +
in-progress guard.

### 4.4 Reader-side note

`build_podcast_update` currently reaches `handle.<slot>.lock()`. After the refactor it reaches
`handle.state.<substate>.<slot>.lock()`. To keep `snapshot.rs` readable and the **byte output
identical**, each substate exposes a thin `project_*` returning the exact same owned value the
snapshot builds today (e.g. `state.wiki.articles_snapshot() -> Vec<WikiArticle>` = `lock().clone()`).
This is a mechanical rename, not a logic change — the regression test (§6.3) guards it.

---

## 5. Composition root (the new `register.rs`)

The 31-arg constructor and the double-clone vanish. `register.rs` builds **one**
`Arc<PodcastAppState>` and hands clones to both seams.

```rust
pub extern "C" fn nmp_app_podcast_register(app: *mut NmpApp) -> *mut PodcastHandle {
    // ... null check, register_defaults, register_action::<…>() (UNCHANGED) ...

    let app_ref = unsafe { &*app };
    let rev = Arc::new(AtomicU64::new(1));
    let runtime = Arc::new(tokio::runtime::Builder::new_multi_thread()
        .thread_name("podcast-tokio").enable_all().build().expect("tokio runtime"));
    let signal = SnapshotUpdateSignal::new(rev.clone(), app_ref.actor_sender());
    let infra = Infra { rev: rev.clone(), signal: Some(signal.clone()), runtime: runtime.clone() };

    // ONE construction. Each substate seeds its own slots and clones `infra`
    // + shared `store` internally. The 31-arg positional ctor is GONE.
    let state = Arc::new(PodcastAppState::new(infra.clone()));

    // Observers register against the SAME substate slots (share() the Arc):
    app_ref.register_event_observer(Arc::new(
        NostrDiscoveryObserver::new(state.discovery.nostr_results.share(), rev.clone())
            .with_snapshot_signal(signal.clone())));
    app_ref.register_event_observer(Arc::new(
        CommentsObserver::new(state.library.store.clone(), state.comments.cache.share(), rev.clone())
            .with_snapshot_signal(signal.clone())));
    // ... agent_notes, feedback observers — same pattern ...

    // Relay seed (UNCHANGED). Feedback runtime now lives in state.feedback.
    app_ref.set_initial_relays_for_start(vec![ /* ... + state.feedback.config().relay_seed() */ ]);

    // Writer seam: ONE Arc clone.
    app_ref.set_host_op_handler(Arc::new(PodcastHostOpHandler::new(app, state.clone())));

    // Reader seam: ONE Arc clone + the two reader-only caches.
    let handle = Arc::new(PodcastHandle {
        app,
        state,                                  // <-- the single shared field
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(HashMap::new())),
        snapshot_signal: Some(signal.clone()),  // kept for the snapshot path's bump
    });

    // Snapshot projection (UNCHANGED — still build_snapshot_payload(&proj)).
    { let proj = Arc::clone(&handle);
      app_ref.register_snapshot_projection("podcast.snapshot",
          move || serde_json::from_str(&build_snapshot_payload(&proj)).unwrap_or(Value::Null)); }

    Arc::into_raw(handle) as *mut PodcastHandle
}
```

New seam structs:
```rust
pub struct PodcastHandle {
    pub(super) app: *mut NmpApp,
    pub(super) state: Arc<PodcastAppState>,
    pub(super) snapshot_cache: Arc<Mutex<Option<(u64, String)>>>,  // reader-only
    pub(super) clean_html_cache: Arc<Mutex<HashMap<u64, String>>>, // reader-only
    pub(crate) snapshot_signal: Option<SnapshotUpdateSignal>,
}
pub struct PodcastHostOpHandler {
    pub(crate) app: *mut NmpApp,
    pub(crate) state: Arc<PodcastAppState>,
}
```

`voice_conversation` (handle-only, holds its own Arcs of store/voice_state/runtime) moves to
`state.voice.conversation` since it is logically the Voice substate's manager; the
`unregister` shutdown call becomes `reclaimed.state.voice.conversation.shutdown()`.

**Reader access pattern**: `build_podcast_update(handle)` reads `handle.state.<substate>.project_*()`.
**Writer access pattern**: the router calls `self.state.<substate>.handle(action)`.
**Off-actor writers** (report FFIs, observers, tokio tasks) call `<substate>.<slot>.share()` to get
the bare `Arc<Mutex<_>>` they already use today — zero change to their locking.

---

## 6. Lock boundaries, ordering, and guardrails

### 6.1 Lock granularity (justified per the review)

**Keep per-substate-slot locks. Do NOT introduce one mega-lock.** Rationale from §2.1: the
snapshot read (actor thread) runs concurrently with report-thread writes (`store`, `player`,
`queue`, `download_queue`) and tokio writebacks (`picks`, `categories`, `inbox_triage_cache`,
`conversation`). A single `Mutex<PodcastAppState>` would serialize all of them against every
snapshot rebuild — re-introducing exactly the main-thread contention PR #264/#267 removed.

The `Slot<T, D>` wrapper preserves *today's exact lock topology* — it is still one `Mutex` per slot.
The composition changes *ownership/addressing*, not *granularity*. This is critical: **the refactor
must not change which locks exist**, only where they hang.

### 6.2 Lock-ordering hierarchy (deadlock prevention)

Consolidating ~30 locks under one tree raises the theoretical risk of a new lock-order inversion.
Today the code mostly takes one lock at a time; `snapshot.rs` is the notable multi-lock reader and
already documents "snapshot caches before the store lock so we don't hold two locks at once"
(handle.rs / snapshot.rs L45). We codify the existing discipline as an explicit, documented order:

**Canonical lock order (acquire in this order, release in reverse; never hold two across a `.await`/bump):**

```
1. library.store         (the persisted root — widest)
2. library.identity
3. playback.{player, queue, downloads}
4. <feature substate slots>   (knowledge.index, wiki.articles, picks, categories, inbox.*, …)
5. reader-only caches    (snapshot_cache, clean_html_cache)
```

Enforcement guardrails (no half-migrated hack):
- **Snapshot builder keeps its "snapshot-then-release" pattern**: clone each slot's value under its
  own lock, drop the guard, *then* take the next — never nest. (Already true today; the refactor
  must preserve it, asserted by review of `build_podcast_update` at each step.)
- **No method holds a `Slot` guard across `infra.bump()`** (bump sends on the actor channel; holding
  a lock across it risks priority inversion with the actor). Existing handlers already drop before
  bump (`download_report.rs` L125–129); we make it a documented rule on `Infra::bump`.
- **No method holds a `Slot` guard across `runtime.block_on` / `runtime.spawn`.** Wiki's
  `block_on(synthesize_summary)` already clones out first; keep that.
- A `debug_assert`-based lightweight lock-order checker (thread-local "highest lock level held")
  can be added in the `Slot::lock` path under `#[cfg(debug_assertions)]` to catch inversions in
  tests/CI without release cost. Recommended but optional.

### 6.3 PodcastUpdate-bytes-identical regression test (the spine of the migration)

This is the **invariant every step must preserve**, and it must exist *before* step 1.

- Add `src/ffi/snapshot_golden_tests.rs`: build a `PodcastAppState` seeded with a fixed, non-trivial
  fixture (a few podcasts/episodes, some downloads, a wiki article, picks, triage entries, comments,
  voice state, etc.), call `build_snapshot_payload`, and assert the JSON **string** equals a checked-in
  golden file `tests/fixtures/snapshot_golden.json`.
- The golden is captured from `main` (pre-refactor) **once**, before any state move. Every migration
  step must leave it byte-identical. A diff = a regression in that step, caught in CI.
- Because `serde_json::to_string` field order follows struct declaration order in `PodcastUpdate`
  (unchanged by this refactor — we touch *state inventory*, not the projection struct), byte-identity
  is achievable. The refactor explicitly does **not** reorder `PodcastUpdate` fields.
- Complement with the existing `snapshot_tests.rs` (25 tests) and `snapshot_widget_seam_tests.rs`,
  which already exercise the projection; they must stay green at every step.

### 6.4 Keeping durability honest

- The `Slot<T, D>` marker makes "a session slot must not become persisted" a **compile error**:
  `persist()` exists only on `Slot<_, Persisted>`. A reviewer flipping `Session`→`Persisted` is a
  visible, reviewable type change, not a silently-added `self.store.persist()` call.
- A test (`state/durability_tests.rs`) asserts the *count* and *names* of `Persisted` slots, so
  adding/removing a persisted slot requires updating the assertion — a tripwire against accidental
  durability drift.
- The persisted-outside-store trio (`podcast_keys`, `inbox_triage_cache`, `agent_tasks`) is
  documented on each substate with the file it persists to, matching today's behavior exactly.

---

## 7. Migration strategy (ordered, individually-landable, always-green strangler)

**Principle:** introduce `PodcastAppState` *alongside* the god-structs, then move substates one at a
time. At every step boundary the tree either fully owns a substate or doesn't — **no half-migrated
slot**. Each step compiles, passes all tests, and keeps `snapshot_golden.json` byte-identical.

This composes *on top of* the in-flight Point 1 namespace-router (assume it has landed:
`host_op_handler::handle` is `match env.ns => parse one enum => call handle_*_action`). Each step
below converts one `handle_*_action` call site into `self.state.<substate>.handle(...)`.

### Step 0 — Scaffolding (no behavior change)
- Add `src/state/{mod.rs, slot.rs}`: `Slot<T,D>`, `Durability` markers, `Infra`, and an **empty**
  `PodcastAppState` holding only `infra`.
- Add the golden test (§6.3) and capture `snapshot_golden.json` from current `main`.
- `register.rs` constructs the (empty) `Arc<PodcastAppState>` and stores it on **both** seams as a
  new field *in addition to* the existing mirrored fields. Nothing reads it yet.
- **Green invariant:** identical bytes; both structs still hold their old fields; new field unused.
- Lands as PR #1. Now the home exists.

### Step 1 — Knowledge (the pattern-setter; lowest risk, fully self-contained)
Chosen first because: 2 slots, actor-thread-only writes, no off-actor writer, no persistence, one
call site, existing direct unit tests. It exercises the *entire* pattern (substate struct, `handle`
method, reader projection, observer-free) with the least blast radius.
- Create `KnowledgeState` owning `results` + `index` (move the two `Arc`s *into* it; `register.rs`
  seeds them there instead of as locals).
- Both god-structs **drop** `knowledge_search_results` + `knowledge_store`; readers/writers reach
  `state.knowledge.*`.
- Convert `handle_knowledge_action` free fn → `KnowledgeState::handle` (§4.1). Update the router
  call site. Update `snapshot.rs` read to `state.knowledge.results_snapshot()`.
- Migrate `knowledge.rs` unit tests to `KnowledgeState::for_test`.
- **Green invariant:** golden byte-identical; knowledge slots exist in exactly one place.
- PR #2. This PR is the **template** every subsequent feature PR copies.

### Steps 2–N — one feature per PR, lowest-coupling first
Order chosen to defer the cross-thread / persisted / shared-slot cases until the pattern is proven:

2. **Wiki** (`articles`, `search_results`) — note it shares `knowledge.index`; pass the shared `Arc` in. Kills the `_with_signal` fork.
3. **Picks** (`picks` + `score_in_progress`) — folds the writer-only guard inside; converts `auto_refresh_picks`.
4. **Categories** (`categories` + `categorization_in_progress`) — converts `auto_categorize`; note the off-actor `feed_fetch`/tokio writers `.share()` the slot.
5. **Clips**, **Transcripts** — trivial single-slot session caches.
6. **Tasks** (`agent_tasks`) — persisted via store; substate holds the cache + `store` Arc.
7. **Inbox** (`dismissed`, `triage_cache`, `triage_in_progress`) — has tokio writers + the snapshot-path proactive trigger; `maybe_enqueue_triage` becomes `state.inbox.maybe_enqueue_triage()`.
8. **Comments** (`comments_cache`, `viewed_comments_episode_id`) — has an observer writer; observer `.share()`s the slot.
9. **Discovery** (`search_results`, `nostr_results`) — observer writer (discovery).
10. **Social** (`social`, `agent_notes`) — observer + tokio writers.
11. **AgentChat** (`conversation`, `agent_busy`, `agent_touched`) — wraps the existing `AgentChatHandler` (which already composes these Arcs) as `state.agent_chat`.
12. **Voice** (`voice_state` + `voice_conversation`) — moves the conversation manager + the `unregister` shutdown call.
13. **Publish** (`podcast_keys` [persisted, own file], `publish_state`).
14. **Playback** (`player_actor`, `queue`, `download_queue`) — **the cross-thread one, done late on purpose**: the report FFIs (`audio_report`, `download_report`, `http_report`) write here. They switch to `handle.state.playback.player.share()` / `.lock()`. Most delicate; done after the pattern is battle-tested.
15. **Library** (`store`, `identity`) — last, since *everything* borrows `store`. By now every substate already holds its `store: Arc<Mutex<PodcastStore>>` clone; this step just relocates the *owner* from a `register.rs` local into `state.library`, and points report-FFI/observer `store` clones at `state.library.store`.
16. **Move `feedback` + `feed_fetch`** into `state` (mechanical — they're already `Arc` newtypes).

### Step N+1 — Collapse the shells
- Both god-structs now hold only `app`, `state`, and (handle) the two reader-only caches +
  `snapshot_signal`. Delete `PodcastHostOpHandler::new`'s remaining params; it becomes
  `new(app, state)`. Delete the `#[allow(clippy::too_many_arguments)]`. Delete the now-empty
  mirrored field declarations.
- `register.rs` is now the ~40-line composition root of §5.
- **Green invariant:** golden byte-identical; `register.rs` has zero `.clone()` pairs; no positional ctor.

### Per-step invariants (apply to EVERY step)
1. Compiles with no new `clippy` allows.
2. `cargo test` green (incl. the 25 snapshot tests + widget seam tests + the feature's own tests).
3. `snapshot_golden.json` byte-identical (CI-enforced).
4. The migrated slot exists in **exactly one** place (god-struct field removed in the *same* PR it's added to the substate — no overlap window).
5. Off-actor writers (reports/observers/tasks) use `.share()` — their lock granularity unchanged.
6. Lock-order hierarchy (§6.2) respected; no guard held across `infra.bump()` / `runtime.*`.

Because each step removes the old field in the same PR it adds the substate, there is **never** a
half-migrated state at a step boundary — satisfying the "each step must be PROPER" constraint.

---

## 8. Risks & mitigations (summary)

| Risk | Mitigation |
|---|---|
| Snapshot bytes drift during a move | §6.3 golden test, captured pre-refactor, CI-gated every step. We do **not** reorder `PodcastUpdate` fields. |
| New lock-order inversion from consolidation | §6.2 explicit hierarchy + "clone-then-drop before next lock" preserved + optional `debug_assert` lock-level checker. |
| Snapshot-read vs writer contention regression | §6.1: `Slot` preserves today's exact per-slot lock topology — no mega-lock. Granularity is unchanged by construction. |
| Session slot silently becomes persisted | §6.4: `persist()` only on `Slot<_,Persisted>` → unrepresentable; durability-count tripwire test. |
| Concurrent agents on hot shared codebase | §7 strangler: 16 small PRs, each self-contained to one feature dir + its 1 router line + its 1 snapshot read line. Merge conflicts are localized to one substate per PR. |
| Off-actor report FFI breakage (audio/download) | Playback migrated *late* (step 14), after the pattern is proven; `.share()` keeps the exact `Arc<Mutex<_>>` the reports already lock. |
| `*mut NmpApp` Send/Sync soundness | Unchanged: `app` stays a per-seam field (NOT in shared state); the two `unsafe impl Send/Sync` blocks and their caller-contract docs move verbatim. |

---

## 9. Upstream (`nmp-core`) vs app-local

**Everything here is app-local.** No `nmp-core` change is required: the FFI wire contract
(`nmp_app_podcast_snapshot`, the `podcast.snapshot` projection, the report FFIs) is untouched; the
host-op `HostOpHandler` trait impl is unchanged in signature. `Slot`/`Durability`/`PodcastAppState`
live entirely under `src/state/`.

*Optional future* upstream improvement (explicitly **not** part of this refactor): nmp-core could
offer a generic `ComposedAppState` + `Slot` primitive so other NMP apps share the durability-tagged
pattern. Prefer proving it app-local first; promote later if a second app wants it.

---

## 10. Out of scope (named future work)

- **Splitting `PodcastStore`** into per-feature persisted substates (settings vs library vs
  positions vs credentials vs triage). It owns the cross-language `podcasts.json` fixture contract
  and a Swift settings mirror; decomposing it is a separate, larger effort. This design deliberately
  keeps it as the canonical persisted island (§3.4) so the two refactors don't collide.
- The `wiki_articles` "logically derived but currently session" reclassification (it's marked
  `Session` here to stay byte-identical; promoting to `Derived` with regeneration is a follow-up).
- Relay-config C-ABI persistence (already tracked in BACKLOG, unrelated).
```
