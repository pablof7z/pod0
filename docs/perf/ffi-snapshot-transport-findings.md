# FFI snapshot transport — measured findings (2026-06-06)

Question: is the FFI bridge making the iOS app sluggish, and does the whole app
state have to cross it on every change?

## Method
Two harnesses, measuring the transport lane only (serialize → JSON bytes →
decode), NOT the `build_podcast_update` field-mapping/`clean_html` cost (that is
the separate `fix/snapshot-strip-html-memo` lane).

- Rust: `apps/nmp-app-podcast/tests/snapshot_transport_perf.rs` (release, host) —
  payload bytes + serde serialize/deserialize.
- Swift: `AppTests/Sources/SnapshotDecodeTransportPerfTests.swift` (iPhone 16
  simulator) — the real `JSONDecoder` with `.convertFromSnakeCase`, exactly as
  `PodcastHandle.podcastSnapshot()` uses.

Realistic field sizes: ~640B description + ~230B summary (1/3 of eps) + urls per
episode; 20 shows.

## Numbers

| Library      | Payload  | Rust serialize | serde decode | **Swift JSONDecoder (sim)** |
|--------------|----------|----------------|--------------|-----------------------------|
| 1,000 eps    | 1.1 MB   | 0.5 ms         | 0.6 ms       | **10.2 ms**                 |
| 3,600 eps    | 3.9 MB   | 1.9 ms         | 2.0 ms       | **35.5 ms**                 |
| 10,000 eps   | 10.9 MB  | 6.8 ms         | 5.7 ms       | **93.8 ms**                 |

Device is ~2-4× slower than the simulator → ~70-140 ms decode for 3,600 eps on a
real iPhone.

### Amplification
A single `mark-played` (one field on one episode, ~1.2 KB of real change)
re-serializes, re-copies, and re-decodes the **entire 3.9 MB** library —
**~3,286× write amplification**.

## Where it hurts (consumer trace)
- `KernelModel.dispatch()` (every user action: mark-played, star, subscribe…)
  → `pullPodcastSnapshotIfChanged(synchronous: true)`
  → `kernel.podcastSnapshot()` decodes the full library **inline on the MainActor**.
  The `synchronous` flag only governs the *hashing*; the **decode is always inline**.
  ⇒ every user action eats a 35 ms+ (sim) / ~100 ms (device) main-thread stall.
- The global `rev` atomic is bumped at 30+ sites — agent chat, social, downloads,
  voice, position, identity — none of which change the library. Each bump makes
  `pullPodcastSnapshotIfChanged` re-decode the full 4 MB library; the Swift
  `libraryMetaHash` gate then *discards* the result (no reassignment), but the
  decode cost was already paid. So the library is re-decoded constantly during
  playback/agent/social activity for no benefit.

## Already-mitigated (do not redo)
- Rust `snapshot_cache` rev-gates re-serialization (skips serialize when rev
  unchanged).
- Swift `lastProcessedRev` drops redundant frames; `libraryMetaHash` /
  `snapshotContentHash` exclude volatile position/buffering; push-path hashing is
  offloaded off-MainActor; `libraryGeneration` gives an O(1) no-op projection
  fast path; narrow `downloads_rev`/`downloads_snapshot` keeps 1 Hz progress off
  the full-library path.
- In flight by peers: `fix/snapshot-strip-html-memo` (within-Rust-serialize),
  `feat/ffi-perf-metrics` (instrumentation), `applyKernelState` no-op guard
  (within-Swift-project). None touch the full-payload decode.

## Conclusion
The sluggishness is real and concentrated in the **full-library JSON decode on
the Swift side**, paid on every `rev` bump — including the many bumps that do not
change the library, and inline on the MainActor for every user action.

## Fix shipped here — move the decode off the MainActor
`KernelModel.pullPodcastSnapshotIfChanged()` previously called
`kernel.podcastSnapshot()` (the full-library `JSONDecoder` pass) **inline on the
MainActor** for every user dispatch, so each mark-played/star/subscribe ate a
~35 ms (sim) / ~100 ms (device) main-thread stall. It now decodes on a dedicated
serial `snapshotDecodeQueue` and hops back to the MainActor to commit — the same
shape the push path already used (its decode runs on the kernel C-callback
thread). The dead `synchronous` parameter on `pullPodcastSnapshotIfChanged` /
`applyPodcastUpdate` (and its inline-hashing branch) was removed; the decode AND
the O(N×M) hashing now always run off-main.

**Why it's safe:** no caller reads `library` / `podcastSnapshot` / `episodes`
synchronously after `dispatch()` returns — all 122 dispatch sites are
fire-and-forget over `@Observable`, so the one-runloop-later commit is invisible.
`nowPlaying` / `snapshot` (live player position) are still assigned on the
MainActor the moment a frame is accepted. The rev-monotonic guards
(`update.rev > lastProcessedRev`, `commitPodcastProjection`'s
`frameRev == lastProcessedRev`) keep out-of-order decodes newest-wins.

**Net:** every user action returns immediately; the multi-MB decode no longer
blocks the UI. CPU is unchanged (it still decodes 4 MB) — that's the follow-up.

### Verification
- `xcodebuild build` succeeds under `SWIFT_STRICT_CONCURRENCY=complete`.
- App launches and renders on a clean iPhone 16 Pro simulator with the change in
  place (an unrelated, in-flight dylib-`@rpath` packaging bug — the app linked a
  peer worktree's dylib by absolute path — had to be worked around to launch; it
  is not caused by this change).

## Follow-up (NOT in this change — separate, larger, wire-contract work)
- **Skip the decode when the library hasn't changed.** The global `rev` bumps on
  non-library events (agent/social/voice/position/downloads); each forces a full
  4 MB decode that the Swift `libraryMetaHash` gate then discards. A dedicated
  `library_rev` would let the pull skip the decode entirely on those ticks. There
  is no single library-mutation choke point today (79 `&mut self` store methods,
  185 lock sites), so this needs a deliberate rev-tracking pass.
- **Delta/narrow projection** (decode only changed rows) — extend the proven
  `downloads_rev` + `downloads_snapshot` pattern to the library. Cuts the 3,286×
  write-amplification of a single-field change. Touches the D5 wire contract.

Rust stays the single source of truth throughout; all of this is transport, not
ownership.
