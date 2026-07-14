# Lane 6 — Embeddings + sqlite-vec Vector Index + Chunking

Branch: `worktree-agent-aa35b44ee3ee3c9d9`

## What shipped

- **SPM dep**: `https://github.com/jkrukowski/SQLiteVec` at version `0.0.14`
  added to `Project.swift`. The `SQLiteVec` library product is linked into
  the `Podcastr` app target. `tuist generate` resolves cleanly; the SQLite
  fallback path was **not** taken.
- `App/Sources/Knowledge/Chunk.swift` — value types: `Chunk`, `ChunkScope`,
  `ChunkMatch`. `Chunk` follows the task contract: `startMS`/`endMS` as
  integer milliseconds, `speakerID: UUID?`, no embedding field (embeddings
  are an internal column of the vector store).
- `App/Sources/Knowledge/ChunkBuilder.swift` — sliding-window chunker
  (target 400 tok, 15% overlap, ±20% snap-to-speaker-turn tolerance) that
  consumes anything conforming to a small `TranscriptLike` /
  `TranscriptSegment` protocol pair (so we don't depend on Lane 5's exact
  struct).
- `App/Sources/Knowledge/VectorIndex.swift` — `VectorStore` protocol +
  production `actor VectorIndex` backed by `SQLiteVec`. Schema:
  `chunks_meta` (ordinary table for FK filters) + `chunks_vec` (`vec0`
  virtual table, FLOAT[1024] cosine) + `chunks_fts` (`fts5` over `text`).
  Disk path: `$applicationSupport/podcastr/vectors.sqlite`, with the
  `podcastr/` subdirectory created on demand (iOS doesn't auto-create it).
- `App/Sources/Knowledge/InMemoryVectorStore.swift` — fallback in-memory
  `VectorStore` implementation (cosine over `[Float]`, Jaccard for the
  hybrid lexical leg, RRF for fusion). **Built but not used as the primary
  path** — kept for tests/previews and as an offline-build escape hatch.
- `App/Sources/Knowledge/EmbeddingsClient.swift` — `OpenRouterEmbeddingsClient`
  posts to `https://openrouter.ai/api/v1/embeddings`, model
  `openai/text-embedding-3-large`, dimensions `1024` (Matryoshka).
  Auth via existing `OpenRouterCredentialStore`. Internal batching at
  100 inputs/request, in-order reassembly.
- `App/Sources/Knowledge/RerankerClient.swift` — `OpenRouterRerankerClient`
  posts to `https://openrouter.ai/api/v1/rerank`, model
  `cohere/rerank-v3.5`. Returns reordered candidate indices.
- `App/Sources/Knowledge/RAGSearch.swift` — orchestrator: query →
  `embedder.embed([query])` → `store.hybridTopK(...)` (or `topK`) →
  optional rerank → top-K `ChunkMatch[]`. Two pre-tuned profiles:
  `.voice` (no rerank, ~180ms target) and `.chat` (with rerank, ~400ms
  target). Reranker failure degrades gracefully to the hybrid order.
- `App/Sources/Knowledge/ChunkHighlights.swift` — shared highlight
  computation reused by both vector stores (so the in-memory fallback
  doesn't have to compile against the SQLiteVec-importing file).
- `AppTests/Sources/ChunkBuilderTests.swift` — 10 unit tests covering
  empty / single-segment / short / long / two-speaker / overlap /
  ID-stability cases. **All 27 app tests pass** (10 new + 17 existing).

## Public protocol surface

```swift
protocol VectorStore: Sendable {
    func upsert(chunks: [Chunk]) async throws
    func deleteAll(forEpisodeID: UUID) async throws
    func topK(_ k: Int,
              for queryVector: [Float],
              scope: ChunkScope?) async throws -> [ChunkMatch]
    func hybridTopK(_ k: Int,
                    query: String,
                    queryVector: [Float],
                    scope: ChunkScope?) async throws -> [ChunkMatch]
}

protocol EmbeddingsClient: Sendable {
    func embed(_ texts: [String]) async throws -> [[Float]]
}

protocol RerankerClient: Sendable {
    func rerank(query: String, documents: [String], topN: Int?) async throws -> [Int]
}

struct RAGSearch: Sendable {
    init(store: VectorStore, embedder: EmbeddingsClient, reranker: RerankerClient?)
    func search(query: String,
                scope: ChunkScope? = nil,
                options: Options = .chat) async throws -> [ChunkMatch]
}
```

Construction recipe used by the rest of the app:

```swift
let embedder = OpenRouterEmbeddingsClient()
let reranker = OpenRouterRerankerClient()
let store    = try VectorIndex(embedder: embedder)
let rag      = RAGSearch(store: store, embedder: embedder, reranker: reranker)
```

## Where Lanes 7 + 10 hook in

- **Lane 7 (LLM Wiki)** writes its outputs into the same `VectorStore`. After
  generating a wiki page, call `store.upsert(chunks: wikiChunks)` where each
  `Chunk` carries the wiki page UUID in `episodeID` (Lane 7 can choose
  whether to overload that field or add a sibling table — recommended: a
  separate `wiki` index built by composing a second `VectorIndex` instance
  with a different on-disk path, mirroring the research note's
  `chunks_transcript` / `chunks_wiki` split).
- **Lane 10 (`query_transcripts` / `query_wiki` agent tools)** call
  `RAGSearch.search(query:scope:options:)`. For voice-mode tool calls use
  `Options.voice`; for chat use `Options.chat`. The `scope` argument lets
  the agent narrow to a specific podcast/episode/speaker UUID without
  changing the call site.

## Decisions / deviations from the research note

- **Chunk shape follows the task spec, not the research doc.** The research
  doc proposes `startSec: Double`, `speaker: String`, embedding-on-Chunk;
  the task fixes `startMS: Int`, `speakerID: UUID?`, embedding-off-Chunk.
  The task wins. Lane 5's `Transcript` will need to expose
  millisecond-grained segment timestamps.
- **`VectorStore.upsert(chunks:)` takes plain `[Chunk]`, not
  `[(Chunk, [Float])]`.** The store owns its embedder. This matches the
  spec literally and keeps the agent-tool / wiki-indexer call sites
  vector-free.
- **No `db.transaction { }` from SQLiteVec.** Its closure parameter isn't
  `@Sendable`, so under Swift 6 strict concurrency the captures across the
  actor boundary fail to compile. Replaced with explicit `BEGIN/COMMIT/
  ROLLBACK` SQL — same atomicity, compiles clean.
- **`Database.Location` not exposed in `VectorIndex.init`.** The enum isn't
  `Sendable` in SQLiteVec 0.0.14, which trips the actor boundary. Init
  takes `fileURL: URL?` + `inMemory: Bool` instead and constructs
  `Location` internally.
- **Highlights via simple substring matching, not FTS5 `offsets()`.** The
  SQLiteVec 0.0.14 binding doesn't expose offset helpers; we approximate
  with case-insensitive token matching (≥3-char tokens). Acceptable for
  the snippet UI; can be upgraded later without changing callers.
- **SQLiteVec `vec0` doesn't accept `WHERE` predicates against
  non-indexed columns**, so scope filtering is a two-stage query: ask
  `vec0` for an over-fetched candidate window, intersect with
  `chunks_meta` filtered by `episode_id` / `podcast_id` / `speaker_id`.
  Default over-fetch is `max(k * 4, k + 16)`.

## Lane 5 status note

At time of writing, `App/Sources/Transcript/Transcript.swift` is still the
empty stub from the scaffold commit. Lane 6 deliberately defines its own
`TranscriptLike` / `TranscriptSegment` protocols inside `ChunkBuilder.swift`
so the build doesn't depend on Lane 5 landing first. When Lane 5 lands its
real `Transcript` struct, conform it to `TranscriptLike` (or add an
adapter) — the chunker doesn't need to change.

## Verification

- `tuist install` and `tuist generate` succeed; SPM resolution of
  `SQLiteVec 0.0.14` completes without errors.
- `xcodebuild` for the iPhone 17 simulator: **build succeeds** for the
  `Podcastr` scheme (Debug configuration).
- Test suite on iPhone 17 sim: **27/27 passing**, including the 10 new
  `ChunkBuilderTests`.
- All Knowledge files are under the 500-line hard limit
  (`VectorIndex.swift` is the largest at 466 lines; the rest are under the
  300-line soft limit).

## Files touched

```
Project.swift                                            (SPM dep added)
App/Sources/Knowledge/Chunk.swift                        (new)
App/Sources/Knowledge/ChunkBuilder.swift                 (new)
App/Sources/Knowledge/ChunkHighlights.swift              (new)
App/Sources/Knowledge/EmbeddingsClient.swift             (new)
App/Sources/Knowledge/InMemoryVectorStore.swift          (new, fallback)
App/Sources/Knowledge/RAGSearch.swift                    (new)
App/Sources/Knowledge/RerankerClient.swift               (new)
App/Sources/Knowledge/VectorIndex.swift                  (replaced stub)
AppTests/Sources/ChunkBuilderTests.swift                 (new)
docs/spec/work-reports/lane-06-rag.md                    (this report)
```

Untouched: everything in `App/Sources/Audio/`, `App/Sources/Podcast/`,
`App/Sources/Features/`, `App/Sources/Transcript/`, `App/Sources/Voice/`,
`App/Sources/Briefing/`, `App/Sources/Agent/AgentTools+Podcast.swift`,
`App/Resources/Info.plist`.
