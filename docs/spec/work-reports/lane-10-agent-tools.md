# Lane 10 — Podcast Agent Tool Surface

> Owner: Engineer — Marcus Webb. Worktree branch: `worktree-agent-a6ef81872ef3a5b26`.
>
> **Scope.** Add the podcast-domain tool-calling surface to the existing
> `AgentTools` extension pattern. **No** existing file in `App/Sources/Agent/`
> was modified; this lane is pure additions in new files.

---

## 1. Summary

This lane delivers ten new agent tools as **pure extensions** of the existing
`AgentTools` enum. The orchestrator wires them into the live `AgentChatSession`
loop at merge time via three integration points (Section 6).

Critical design decision: because `AgentTools.dispatch` lives in a read-only
file with a hard `default:` error path, the podcast tools are routed through
a **separate** `dispatchPodcast(name:argsJSON:deps:)` entry point that takes an
**explicit** `PodcastAgentToolDeps` parameter — no singleton, no global state.
The orchestrator decides at wire-up time how to source `deps` and how to fan
out the main switch into `dispatchPodcast`.

All ten tools are unit-tested (29 test cases) against actor-based mocks; the
test suite runs in ~100 ms and is independent of `AppStateStore`.

---

## 2. Files added

| Path | Lines | Purpose |
| --- | --- | --- |
| `App/Sources/Agent/PodcastAgentToolDeps.swift` | 260 | Value types + 6 protocols + the `PodcastAgentToolDeps` aggregate. |
| `App/Sources/Agent/PerplexityClient.swift` | 164 | `actor` implementation of `PerplexityClientProtocol`. Reads bearer token from `KeychainStore` under `(service: "PerplexityCredentialStore", account: "perplexity_api_key")`. |
| `App/Sources/Agent/AgentTools+Podcast.swift` | 421 | `AgentTools.PodcastNames` enum, `dispatchPodcast` entry points, and the ten tool handlers. |
| `App/Sources/Agent/AgentToolSchema+Podcast.swift` | 145 | `AgentTools.podcastSchema` (OpenAI-compatible array). |
| `AppTests/Sources/AgentToolsPodcastTests.swift` | 444 | 29 unit tests covering schema validation + dispatch happy/sad paths. |
| `AppTests/Sources/AgentToolsPodcastMocks.swift` | 124 | Actor-based mocks for every protocol in `PodcastAgentToolDeps`. |

All files satisfy the AGENTS.md hard cap (500 lines).

**No modifications** to any existing file in `App/Sources/Agent/` or anywhere
else in the repo. `Project.swift` is unchanged — Tuist already globs
`App/Sources/**` and `AppTests/Sources/**`, so the new files compile in
automatically.

---

## 3. Tool catalogue

For each tool: signature → schema entry → dispatch path → protocol method.

### 3.1 `play_episode_at`
- **Signature.** `play_episode_at(episode_id: string, timestamp: number) → { episode_id, timestamp, episode_title?, podcast_title?, duration_seconds? }`
- **Schema.** `AgentToolSchema+Podcast.swift` — required `episode_id`, `timestamp`. `timestamp` validated `>= 0`.
- **Dispatch.** `AgentTools.playEpisodeAtTool` → `EpisodeFetcherProtocol.episodeExists` (validate) → `PlaybackHostProtocol.playEpisodeAt` → `EpisodeFetcherProtocol.episodeMetadata` (decorate response).
- **Lane.** Lane 1 (Audio/Player) implements `PlaybackHostProtocol`; Lane 2 (Podcast/Episode) implements `EpisodeFetcherProtocol`.

### 3.2 `search_episodes`
- **Signature.** `search_episodes(query: string, scope?: string, limit?: int=10) → { query, total_found, results: [EpisodeHit] }`
- **Schema.** Required `query`. `limit` clamped to `[1, 25]`.
- **Dispatch.** `AgentTools.searchEpisodesTool` → `RAGSearchProtocol.searchEpisodes`.
- **Lane.** Lane 4 (Knowledge/RAG).

### 3.3 `query_wiki`
- **Signature.** `query_wiki(topic: string, scope?: string, limit?: int=5) → { topic, total_found, results: [WikiHit] }`
- **Schema.** Required `topic`. `limit` clamped to `[1, 10]`.
- **Dispatch.** `AgentTools.queryWikiTool` → `WikiStorageProtocol.queryWiki`.
- **Lane.** Lane 5 (Knowledge/Wiki).

### 3.4 `query_transcripts`
- **Signature.** `query_transcripts(query: string, scope?: string, limit?: int=8) → { query, total_found, results: [TranscriptHit] }`
- **Schema.** Required `query`. `limit` clamped to `[1, 25]`. `scope` may be an `episode_id` or `podcast_id`.
- **Dispatch.** `AgentTools.queryTranscriptsTool` → `RAGSearchProtocol.queryTranscripts`.
- **Lane.** Lane 4 (RAG / Transcripts).

### 3.5 `generate_briefing`
- **Signature.** `generate_briefing(scope: string, length: int, style?: "news"|"deep_dive"|"quick_hits") → { briefing_id, title, estimated_seconds, episode_ids, scope, length_minutes, style?, script_preview? }`
- **Schema.** Required `scope`, `length`. `length` clamped to `[3, 30]` minutes.
- **Dispatch.** `AgentTools.generateBriefingTool` → `BriefingComposerProtocol.composeBriefing`.
- **Lane.** Lane 8 (Briefing).

### 3.6 `perplexity_search`
- **Signature.** `perplexity_search(query: string) → { query, answer, sources: [{title, url}] }`
- **Schema.** Required `query`.
- **Dispatch.** `AgentTools.perplexitySearchTool` → `PerplexityClientProtocol.search`.
- **Lane.** Lane 9 (Web / online search). Default impl `PerplexityClient` ships with this lane and reads the API key from `KeychainStore` (see Section 5).

### 3.7 `summarize_episode`
- **Signature.** `summarize_episode(episode_id: string, length?: "short"|"medium"|"long") → { episode_id, summary, bullets?, length? }`
- **Schema.** Required `episode_id`.
- **Dispatch.** `AgentTools.summarizeEpisodeTool` → `EpisodeFetcherProtocol.episodeExists` (validate) → `EpisodeSummarizerProtocol.summarizeEpisode`.
- **Lane.** Lane 5 / 8 (Wiki/Summarization).

### 3.8 `find_similar_episodes`
- **Signature.** `find_similar_episodes(seed_episode_id: string, k?: int=5) → { seed_episode_id, k, total_found, results: [EpisodeHit] }`
- **Schema.** Required `seed_episode_id`. `k` clamped to `[1, 20]`.
- **Dispatch.** `AgentTools.findSimilarEpisodesTool` → `EpisodeFetcherProtocol.episodeExists` (validate seed) → `RAGSearchProtocol.findSimilarEpisodes`.
- **Lane.** Lane 4 (RAG).

### 3.9 `open_screen`
- **Signature.** `open_screen(route: string) → { route }`
- **Schema.** Required `route`.
- **Dispatch.** `AgentTools.openScreenTool` → `PlaybackHostProtocol.openScreen`.
- **Lane.** Lane 1 / 9 (UI host / navigation).

### 3.10 `set_now_playing`
- **Signature.** `set_now_playing(episode_id: string, timestamp?: number) → { episode_id, timestamp? }`
- **Schema.** Required `episode_id`. `timestamp`, if present, validated `>= 0`.
- **Dispatch.** `AgentTools.setNowPlayingTool` → `EpisodeFetcherProtocol.episodeExists` (validate) → `PlaybackHostProtocol.setNowPlaying`.
- **Lane.** Lane 1 (Audio/Now Playing).

---

## 4. Protocol surface (`PodcastAgentToolDeps`)

Every protocol below is `Sendable`; concrete implementations are free to mark
methods `@MainActor` if they touch the store or SwiftUI state.

```swift
public protocol RAGSearchProtocol: Sendable {
    func searchEpisodes(query: String, scope: PodcastID?, limit: Int) async throws -> [EpisodeHit]
    func queryTranscripts(query: String, scope: String?, limit: Int) async throws -> [TranscriptHit]
    func findSimilarEpisodes(seedEpisodeID: EpisodeID, k: Int) async throws -> [EpisodeHit]
}

public protocol WikiStorageProtocol: Sendable {
    func queryWiki(topic: String, scope: PodcastID?, limit: Int) async throws -> [WikiHit]
}

public protocol BriefingComposerProtocol: Sendable {
    func composeBriefing(scope: String, lengthMinutes: Int, style: String?) async throws -> BriefingResult
}

public protocol EpisodeSummarizerProtocol: Sendable {
    func summarizeEpisode(episodeID: EpisodeID, length: String?) async throws -> EpisodeSummary
}

public protocol EpisodeFetcherProtocol: Sendable {
    func episodeExists(episodeID: EpisodeID) async -> Bool
    func episodeMetadata(episodeID: EpisodeID) async -> (podcastTitle: String, episodeTitle: String, durationSeconds: Int?)?
}

public protocol PlaybackHostProtocol: Sendable {
    func playEpisodeAt(episodeID: EpisodeID, timestampSeconds: Double) async
    func setNowPlaying(episodeID: EpisodeID, timestampSeconds: Double?) async
    func openScreen(route: String) async
}

public protocol PerplexityClientProtocol: Sendable {
    func search(query: String) async throws -> PerplexityResult
}
```

All identifiers are stringly-typed (`EpisodeID = String`, `PodcastID = String`)
so this lane has zero compile-time coupling to lanes 1–9. The orchestrator's
adapter at merge time is responsible for translating between these strings and
the underlying domain models.

---

## 5. PerplexityClient — Keychain gating

The default `PerplexityClient` actor reads its bearer token via the existing
`KeychainStore.readString` API. The contract is:

| Aspect | Value |
| --- | --- |
| Keychain service | `"PerplexityCredentialStore"` |
| Keychain account | `"perplexity_api_key"` |
| HTTP endpoint | `https://api.perplexity.ai/chat/completions` |
| Default model | `sonar-small-online` |
| Citation support | Both `citations: [String]` and `search_results: [{title,url}]` shapes are tolerated |

**Important caveat.** Per the lane brief, this lane does **not** add a typed
`PerplexityCredentialStore` alongside the existing `OpenRouterCredentialStore`
/ `ElevenLabsCredentialStore`. The TODO in `PerplexityClient.swift` flags that
this should be created by a settings/keychain lane. Until then, the key is
stored raw via `KeychainStore.saveString` and read raw via
`KeychainStore.readString`.

When no key is set, `PerplexityClient.search(query:)` throws
`PerplexityClientError.missingAPIKey` and the `perplexity_search` tool
returns a clean `{"error": "...needs setup"}` envelope to the agent, so the
agent loop can surface a graceful "Perplexity isn't configured" message
rather than crashing.

---

## 6. Orchestrator integration points

The orchestrator owns three pieces of wiring at merge time. None of them
require modifying lane-10 files; each is a single small edit elsewhere.

### 6.1 Schema concatenation

Wherever the agent's tool list is consumed (currently
`AgentChatSession.swift:239` — `tools: AgentTools.schema`), update to:

```swift
tools: AgentTools.schema + AgentTools.podcastSchema,
```

### 6.2 Dispatch routing

Inside `AgentTools.dispatch(name:argsJSON:store:batchID:)`, before the
`default:` arm, add a single case that delegates podcast names to
`dispatchPodcast`:

```swift
case let n where AgentTools.PodcastNames.all.contains(n):
    return await AgentTools.dispatchPodcast(
        name: n,
        argsJSON: argsJSON,
        deps: store.podcastDeps   // see 6.3
    )
```

`AgentTools.PodcastNames.all` is exposed for exactly this purpose.

### 6.3 Deps construction

Add a `podcastDeps: PodcastAgentToolDeps` property to `AppStateStore` (or
inject through `AgentChatSession`'s init). The orchestrator constructs it
once at app startup, supplying:

| Field | Implementer (lane) |
| --- | --- |
| `rag` | Lane 4 — `RAGQueryService` |
| `wiki` | Lane 5 — `WikiStorage` |
| `briefing` | Lane 8 — `BriefingComposer` |
| `summarizer` | Lane 5 / 8 — `EpisodeSummarizer` |
| `fetcher` | Lane 2 — `EpisodeRepository` (probably an `AppStateStore` adapter) |
| `playback` | Lane 1 / 9 — `PlaybackEngine` adapter |
| `perplexity` | This lane — `PerplexityClient()` |

---

## 7. Deliberate non-goals (orchestrator decisions)

1. **Activity log entries.** Mutating tools (`play_episode_at`, `set_now_playing`,
   `open_screen`) currently do **not** record `AgentActivityEntry` rows because
   the deps surface intentionally has no `AppStateStore` dependency. If undo
   parity with the items toolset is wanted, the orchestrator can wrap
   `dispatchPodcast` in a recorder at wire-up — or a future lane can extend the
   protocols to include an activity-recorder.
2. **Friend-agent ACL.** `AgentRelayBridge` currently dispatches every tool
   the agent supports. The orchestrator should audit which podcast tools are
   safe to expose to a friend's DM (`play_episode_at` on someone else's
   device is non-trivially weird). My recommendation: gate
   `play_episode_at` / `set_now_playing` / `open_screen` /
   `generate_briefing` behind an explicit per-friend approval; the read-only
   tools (`search_episodes`, `query_wiki`, `query_transcripts`,
   `summarize_episode`, `find_similar_episodes`, `perplexity_search`) are
   safe by default.
3. **Schema flag for tool gating.** `AgentTools.podcastSchema` returns the
   full set. If a lane wants per-context filtering (e.g. hide
   `perplexity_search` when offline), filter at concatenation time
   (`AgentTools.podcastSchema.filter { … }`) — no need to edit this lane.

---

## 8. Validation

- **Build.** `xcodebuild build-for-testing` against `iPhone 17` simulator → `** TEST BUILD SUCCEEDED **`.
- **Tests.** `AgentToolsPodcastTests` — 29 tests, 0 failures, ~100 ms total. `AppTests` (pre-existing) — 17 tests, 0 failures.
- **Strict concurrency.** Swift 6 with `SWIFT_STRICT_CONCURRENCY=complete` is on. Mocks are `actor`-isolated; production code uses `Sendable` value types at every protocol boundary.
- **File-size discipline.** Largest file in this lane is `AgentTools+Podcast.swift` at 421 lines (under hard 500 cap). Tests file split into `AgentToolsPodcastTests.swift` (444) + `AgentToolsPodcastMocks.swift` (124).
- **No SPM deps added.** `Project.swift` untouched.

---

## 9. Files touched

```
NEW   App/Sources/Agent/PodcastAgentToolDeps.swift
NEW   App/Sources/Agent/PerplexityClient.swift
NEW   App/Sources/Agent/AgentTools+Podcast.swift
NEW   App/Sources/Agent/AgentToolSchema+Podcast.swift
NEW   AppTests/Sources/AgentToolsPodcastTests.swift
NEW   AppTests/Sources/AgentToolsPodcastMocks.swift
NEW   docs/spec/work-reports/lane-10-agent-tools.md
```

Every existing file is untouched.
