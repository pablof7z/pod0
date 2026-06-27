# Optimistic subscribe + async HTTP capability

**Status:** landed on `main`; keep this file as the design record.

Current code has the async HTTP capability namespace/report channel, pending
feed-fetch registry, optimistic subscribe insert, and iOS/Android/TUI/headless
executors wired. Follow-up work belongs in `docs/BACKLOG.md`, not in this plan
as active implementation state.

## Problem

Subscribing to a podcast takes multiple seconds with no feedback. `handle_subscribe`
(`apps/nmp-app-podcast/src/host_op_handler/podcast_actions_feed.rs`) runs synchronously
on the NMP **actor thread**: it builds a feed request, calls the **synchronous** HTTP
capability (`dispatch_http` → `*mut NmpApp::dispatch_capability`, which blocks the actor
thread on the full RSS download per ADR-0023), parses every episode, and only *then*
inserts the podcast row + bumps `rev`. So the row does not appear in the library until
the entire feed is fetched and parsed, and the actor thread is blocked for other actions
the whole time. The Swift side (`AppStateStore+KernelActions.kernelSubscribe`) just
polls the projection every 300 ms until the row shows up.

## Goal

Subscribe is **instant**: the podcast row appears immediately (optimistic insert), and
the feed fetch + episode hydration happens in the background without blocking the actor
thread. Works on **all platforms** (iOS, Android, TUI) because the orchestration lives in
the shared Rust kernel.

## Approach (chosen): async HTTP capability + report-back

Mirror the **download capability**: the kernel emits a fire-and-forget command; the
platform runs the transport off-thread and reports the result back through a dedicated
FFI report channel, which resumes kernel-side processing and bumps `rev`. This keeps the
D7 capability doctrine (platform executes transport, kernel decides) while making the call
non-blocking.

### 1. Wire contract (`apps/podcast-feeds/src/http.rs`) — additive

```rust
pub const HTTP_ASYNC_CAPABILITY_NAMESPACE: &str = "nmp.http.async.capability";

/// Fire-and-forget async HTTP command. The executor runs `request` off-thread
/// and reports back an `HttpReport` carrying the same `request_id`.
pub struct HttpCommand { pub request_id: String, pub request: HttpRequest }

/// iOS/Android/TUI -> kernel async result, keyed by the originating request_id.
pub struct HttpReport { pub request_id: String, pub result: HttpResult }
```

`HttpRequest` / `HttpResult` are reused unchanged (the executor already knows how to run
an `HttpRequest` and build an `HttpResult`).

### 2. Kernel: pending-request registry + continuation

- `PodcastHostOpHandler` gains `pending_http: Arc<Mutex<HashMap<String, PendingFeedFetch>>>`,
  shared with `PodcastHandle` (the FFI report entry lives on the handle).
- `PendingFeedFetch { mode: FeedFetchMode, podcast_id, url, known: bool }`,
  `FeedFetchMode = Subscribe | Refresh | EnsurePodcast`.
- New `dispatch_http_async(&self, request, pending)`:
  1. generate `request_id` (uuid),
  2. insert `pending` into the registry,
  3. fire-and-forget `dispatch_capability(HTTP_ASYNC_CAPABILITY_NAMESPACE, HttpCommand{..})`
     (drop the ack envelope, exactly like `dispatch_download`),
  4. return immediately.
- Extract the post-fetch body of the current `handle_subscribe` /
  `handle_ensure_podcast` / `handle_refresh` into one shared
  `apply_feed_fetch_result(&self, pending, http_result)` that runs the existing
  parse → merge_episodes → store.subscribe/upsert/update_refresh_metadata → `rev`/signal
  bump → auto_categorize/auto_refresh_picks logic.

### 3. Kernel: report FFI (`apps/nmp-app-podcast/src/ffi/http_report.rs`)

`nmp_app_podcast_http_report(handle, report_json) -> *mut c_char` mirroring
`download_report.rs` (degrade-silently / D6, never panic across FFI, return NULL — no
follow-up). It decodes `HttpReport`, removes the matching `PendingFeedFetch` from the
registry, calls `apply_feed_fetch_result`, and returns NULL. Registered in
`ffi/register.rs` and declared in `App/Sources/Bridge/NmpCore.h`
(+ `ios/Podcast/Podcast/Bridge/NmpCore.h`).

### 4. Kernel: `handle_subscribe` becomes optimistic

1. parse URL, reject already-subscribed (unchanged, synchronous, fast).
2. **optimistic insert**: build a placeholder `Podcast` (feed_url set, `title` = feed host,
   `title_is_placeholder = true`, empty episodes), `store.subscribe(placeholder, vec![])`,
   bump `rev` + signal → row appears on the next projection tick.
   - if the podcast was already *known* (unsubscribed) reuse its row + mark subscribed
     instead of overwriting metadata.
3. `dispatch_http_async(feed_request, PendingFeedFetch{ Subscribe, podcast_id, url, known })`.
4. return `{"ok": true, "status": "subscribing", "podcast_id": ...}` immediately.

`handle_refresh` / `handle_ensure_podcast` switch to the same async dispatch (no optimistic
insert needed for refresh — the row already exists). On a transport **error** report the
placeholder row is kept (user can pull-to-refresh); a `refresh_error` follow-up is filed in
BACKLOG.

### 5. iOS executor (`App/Sources/Capabilities/HttpCapability.swift` + `ios/Podcast/...`)

Add an async path alongside the existing synchronous one (do not change sync semantics —
itunes/transcript/chapters still use it):
- `executeAsync(_ command: HttpCommand)` decodes the command, runs the existing
  `URLSession` data task **without** the blocking semaphore, and on completion calls
  `nmp_app_podcast_http_report(handle, reportJSON)` directly from the completion handler —
  exactly how `DownloadCapability` calls `nmp_app_podcast_download_report` from its delegate.
- Router (`PodcastCapabilities`) dispatches `HTTP_ASYNC_CAPABILITY_NAMESPACE` to
  `executeAsync` and returns an immediate ack envelope.
- `KernelBridge+Callbacks` needs no new report-pull channel (iOS pushes the report via the
  FFI from the URLSession completion, same as downloads); it only needs the handle wired
  into `HttpCapability` so the completion can call the FFI.

### 6. Android + TUI executors

- Android (`apps/nmp-app-podcast/src/android/…` + Kotlin): JNI `nativeHttpReport` +
  Kotlin async HTTP executor that posts the report.
- TUI (`apps/podcast-tui/src/runtime.rs::dispatch_capability_request`): handle the async
  namespace by spawning the HTTP off the actor thread and calling the report path.

## Non-goals / follow-ups (BACKLOG)

- Migrating the *other* `dispatch_http` callers (iTunes search, transcript, chapters, OPML)
  to the async path — they keep the synchronous socket for now.
- Surfacing feed-fetch transport errors on the optimistic row (`refresh_error`).
- Non-UTF-8 RSS body handling (pre-existing, tracked separately).

## Validation

- Rust: `cargo test -p nmp-app-podcast -p podcast-feeds` (new pending-registry + report
  round-trip tests; subscribe optimistic-insert test).
- iOS: focused `xcodebuild test` on the HTTP capability wire tests; manual sim subscribe
  shows the row instantly then episodes fill in.
- TUI: `cargo run -p podcast-tui` subscribe smoke.
