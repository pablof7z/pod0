# Embeddings + RAG Stack for the Podcast Player

> Research note covering OpenRouter's embeddings support, on-device vector storage on iOS, chunking strategy, hybrid lexical+vector search, SwiftData integration, and a cost model. Targets ~50k–200k transcript chunks per power user.

---

## 1. OpenRouter for Embeddings — Yes, It Works

OpenRouter ships an OpenAI-compatible embeddings endpoint at `POST https://openrouter.ai/api/v1/embeddings` and a `/embeddings/models` listing endpoint. Same auth scheme, same SDK shape. ([docs](https://openrouter.ai/docs/api/reference/embeddings), [collection](https://openrouter.ai/collections/embedding-models))

Pricing snapshot (May 2026, input-only because embeddings have no output tokens):

| Model                              | Ctx     | $/M tok | Notes                                               |
| ---------------------------------- | ------- | ------- | --------------------------------------------------- |
| `cohere/embed-v1-0.6b`             | 32K     | $0.004  | Cheapest non-free option                            |
| `qwen3-embedding-8b`               | 32K     | $0.01   | Strong multilingual                                 |
| `baai/bge-m3`                      | 8K      | $0.01   | Solid open weights baseline                         |
| `openai/text-embedding-3-small`    | 8K      | $0.02   | 1536-d default, Matryoshka down to 512              |
| `openai/text-embedding-3-large`    | 8K      | $0.13   | 3072-d default, can request 1024-d via `dimensions` |
| `google/gemini-embedding-001`      | 20K     | $0.15   | 3072-d, multimodal                                  |
| `nvidia/llama-nemotron-embed-vl-1b-v2` | 131K | Free    | Multimodal, 131K context — interesting for full episodes |

Rate limits: paid models on OpenRouter have **no hard rate limit**; OpenRouter routes to whichever upstream has capacity. With BYOK (Bring Your Own Key), the first 1M requests/month are free and after that OpenRouter charges 5% of headline price. ([rate limits](https://openrouter.ai/docs/api/reference/limits), [BYOK](https://openrouter.ai/docs/guides/overview/auth/byok))

**Recommendation:** Default to `openai/text-embedding-3-large` requested at **1024 dimensions** (the `dimensions` API parameter truncates Matryoshka-style without quality loss [OpenAI announcement](https://openai.com/index/new-embedding-models-and-api-updates/)). This gives us best-in-class quality, smaller indexes, and the option to swap to `voyage-3-large` (2048-d, $0.18/M, [Voyage docs](https://docs.voyageai.com/docs/pricing)) if MTEB benchmarks for our domain demand it. Keep the call abstracted behind an `EmbeddingProvider` protocol so we can swap providers without touching call sites.

For reranking, use **Cohere `rerank-v3.5` via OpenRouter** at $0.001/search (4096-tok context, [model page](https://openrouter.ai/cohere/rerank-v3.5)). One pre-paid path covers both stages.

---

## 2. Chunking Strategy for Podcast Transcripts

Transcripts are unstructured spoken word; they need both **time grounding** (so the agent can `play_episode_at(timestamp)`) and **semantic coherence** (so retrieval works).

**Recommended chunker:**

- **Size:** 400–512 tokens (~30–45 seconds of speech).
- **Overlap:** 15% (≈60–80 tokens). Sliding window across turn boundaries — Stack Overflow, Weaviate, and Firecrawl all converge on 200–512 / 10–20% for transcript-style content. ([Stack Overflow](https://stackoverflow.blog/2024/12/27/breaking-up-is-hard-to-do-chunking-in-rag-applications/), [Firecrawl](https://www.firecrawl.dev/blog/best-chunking-strategies-rag))
- **Boundary preference:** snap to speaker-turn boundary when within ±20% of target size. Falls back to sentence boundary, then character.
- **Semantic chunking** (cosine-distance break detection) gives ~+9% recall but doubles the embedding cost during ingest. Use it for **wikis**; use sliding-window for **transcripts** (where speaker turns already act as natural breaks).

**Per-chunk metadata** (this is the critical part for the timestamp-jump UX):

```swift
struct TranscriptChunk {
    let id: UUID
    let episodeID: UUID         // FK to SwiftData Episode
    let podcastID: UUID         // for scope filtering
    let chunkIndex: Int         // ordering within episode
    let startSec: Double        // for play_episode_at
    let endSec: Double
    let speaker: String?        // diarization label
    let text: String            // raw text for display + BM25
    let embedding: [Float]      // 1024-d
    let createdAt: Date
}
```

**One global index** (not per-podcast). Scope filtering happens at query time via SQL `WHERE podcastID IN (...)`. A global index is simpler, supports cross-podcast queries (*"what does any podcast say about Ozempic?"*), and HNSW handles 100K+ chunks fine. Per-podcast indexes only make sense if you regularly want to swap them in/out of memory — we don't.

---

## 3. iOS-Side Vector Storage — Survey

| Option                          | Footprint | Persistence  | 100K-chunk query | Notes                                                                 |
| ------------------------------- | --------- | ------------ | ---------------- | --------------------------------------------------------------------- |
| **`sqlite-vec` via SQLiteVec**  | ~500 KB   | SQLite file  | ~70ms @ 384-d full scan, ~17ms with int8 quant | Brute-force KNN, but composes with FTS5 for hybrid search in one DB file. ([benchmarks](https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/index.html), [Swift binding](https://github.com/jkrukowski/SQLiteVec)) |
| **USearch (`unum-cloud/usearch`)** | ~1 MB | Custom file | <5ms HNSW @ 1M+  | True ANN with HNSW. "Scales to 100M entries on iPhone." ([repo](https://github.com/unum-cloud/usearch), [example](https://github.com/ashvardanian/SwiftSemanticSearch)) |
| **ObjectBox 4.0**               | ~2 MB     | Native DB    | <10ms HNSW       | Full database + HNSW. ORM-like Swift API, but vendor lock-in. ([blog](https://objectbox.io/swift-ios-on-device-vector-database-aka-semantic-index/)) |
| **SVDB / VecturaKit**           | ~200 KB   | JSON/plist   | Linear; OK <10K  | Toy-grade. Skip for our scale. ([SVDB](https://github.com/Dripfarm/SVDB)) |
| **Apple `NLEmbedding`**         | 0 KB      | N/A          | N/A — embedder, not store | 512-d only, **word-level non-contextual** ("bank" same in any sense). `NLContextualEmbedding` exists but isn't a vector index. Useful as a *fallback* embedder offline, not a store. ([docs](https://developer.apple.com/documentation/naturallanguage/nlembedding)) |
| **Hand-rolled `[Float]` in SwiftData** | 0 extra | SwiftData   | 100K × 1024-d ≈ 200ms+ on M-class, more on phones | OK for prototype. Not OK for the marquee experience. |

**Recommendation: `sqlite-vec` (via `jkrukowski/SQLiteVec`).**

Reasons:

1. **One database file** holds chunk text, FTS5 index, and vector index — atomic backups, single migration story.
2. **FTS5 + vec0 hybrid search** is officially blessed and well-documented; trivially supports BM25 + vector + RRF in pure SQL. ([Garcia's hybrid post](https://alexgarcia.xyz/blog/2024/sqlite-vec-hybrid-search/index.html))
3. **Quantization-aware**: int8 brings 100K queries down to ~17ms; bit quantization to ~4ms. Plenty of headroom for 200K+ chunks.
4. **Pre-built iOS binaries** since v0.1.2 ([Garcia's iOS guide](https://alexgarcia.xyz/sqlite-vec/android-ios.html)).
5. Plays nicely with the existing `SQLite.swift` ecosystem if we ever want type-safe queries.

USearch is the runner-up — pick it if benchmarks at 200K+ chunks reveal sqlite-vec brute-force is too slow. ObjectBox is rejected on lock-in grounds.

---

## 4. SwiftData ↔ Vector Store Integration

**Keep them separate. Bridge by UUID.**

- **SwiftData** owns: `Podcast`, `Episode`, `Transcript`, `WikiPage`, user prefs, queue, listen history. Everything that benefits from `@Model`, CloudKit sync, `@Query` live updates.
- **`vectors.sqlite` (sqlite-vec)** owns: `chunks` (rowid + embedding + FTS), nothing else.
- The only shared key is `episodeID: UUID`. SwiftData persists `episodeID.uuidString`; `vectors.sqlite` stores it as a TEXT column.

Why not keep vectors in SwiftData? SwiftData stores `[Float]` as serialized blob, can't index it, can't FTS it, and can't run `vec0` MATCH on it. Performance dies above ~5K chunks. CloudKit sync of 100K embedding blobs would also be a nightmare.

**Sync model:** vectors are **derived data, never synced**. If the user installs on a new device, we re-embed from the SwiftData transcripts on first launch (or pull cached embeddings from a server-side bucket if we add one later). This keeps CloudKit traffic sane.

---

## 5. RAG Pipeline for `query_transcripts`

```
agent calls query_transcripts(query, scope?)
  ├─ embed query with text-embedding-3-large @ 1024d
  ├─ SQL: vec0 MATCH for top-50 (cosine) WHERE podcastID IN scope
  ├─ SQL: FTS5 BM25 for top-50 WHERE podcastID IN scope
  ├─ RRF merge → top-20
  ├─ Cohere rerank-v3.5 → top-5
  └─ return [{ episodeID, startSec, endSec, speaker, text, score }]
```

Latency budget: embed ~150ms (network) + SQL ~30ms + rerank ~200ms (network) ≈ **~400ms**. Acceptable for an agent tool call. If the agent is operating under a "rapid voice" budget we drop the reranker (skip step 4) and live with RRF results: **~180ms total**.

Reciprocal Rank Fusion uses `score = 1/(60 + rank_fts) + 1/(60 + rank_vec)` — no score calibration needed. ([Garcia](https://alexgarcia.xyz/blog/2024/sqlite-vec-hybrid-search/index.html))

---

## 6. Wiki RAG vs Transcript RAG

**Two indexes, same database, same schema.**

- `chunks_transcript` — small, time-anchored, speaker-tagged; sliding window.
- `chunks_wiki` — larger (~1000 tokens), semantic chunks, anchored to wiki section headings.

The agent picks via tool dispatch: `query_wiki(topic)` hits the wiki index, `query_transcripts(query)` hits the transcript index. The orchestrator can call both in parallel for cross-synthesis questions ("*what does this podcast say about Ozempic?*"). Combining at query time is wrong — different chunk sizes confuse RRF; let the agent reason over two retrieved sets.

---

## 7. Refresh / Incremental Indexing

- **New episode transcribed** → diff transcript by `chunkIndex`, embed only new chunks, INSERT into `chunks_transcript`. Idempotent on `(episodeID, chunkIndex)`.
- **Wiki regenerated** → DELETE WHERE `wikiPageID = ?`, re-embed, INSERT.
- **Episode deleted from subscription** → DELETE WHERE `episodeID = ?`. sqlite-vec handles deletes natively (rowid-based).
- **Embedding model changed** → bump `schemaVersion`, full re-embed in background with progress UI. Keep both indexes alive during migration.
- **Batch size:** 100 chunks per OpenAI request (their hard limit; OpenRouter passes through). Use `OperationQueue` with 3 concurrent batches; that's ~300 chunks/sec realistic.

---

## 8. Sample Swift Sketches

**Chunk insertion table:**

```swift
import SQLiteVec

func setupSchema(_ db: Database) async throws {
    try await db.execute("""
        CREATE VIRTUAL TABLE IF NOT EXISTS chunks_transcript USING vec0(
            chunk_id    TEXT PRIMARY KEY,
            episode_id  TEXT,
            podcast_id  TEXT,
            start_sec   FLOAT,
            end_sec     FLOAT,
            embedding   FLOAT[1024]
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
            chunk_id UNINDEXED, text, tokenize='porter'
        );
    """)
}
```

**Embed via OpenRouter:**

```swift
struct OpenRouterEmbedder: EmbeddingProvider {
    let apiKey: String
    let model = "openai/text-embedding-3-large"

    func embed(_ texts: [String]) async throws -> [[Float]] {
        var req = URLRequest(url: URL(string: "https://openrouter.ai/api/v1/embeddings")!)
        req.httpMethod = "POST"
        req.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONEncoder().encode([
            "model": .string(model),
            "input": .array(texts.map(JSONValue.string)),
            "dimensions": .int(1024)
        ] as [String: JSONValue])
        let (data, _) = try await URLSession.shared.data(for: req)
        let decoded = try JSONDecoder().decode(EmbeddingResponse.self, from: data)
        return decoded.data.map(\.embedding)
    }
}
```

**Insert + query:**

```swift
func insert(_ chunk: TranscriptChunk, db: Database) async throws {
    try await db.execute("""
        INSERT INTO chunks_transcript(chunk_id, episode_id, podcast_id, start_sec, end_sec, embedding)
        VALUES (?, ?, ?, ?, ?, ?)
    """, params: [chunk.id.uuidString, chunk.episodeID.uuidString, chunk.podcastID.uuidString,
                  chunk.startSec, chunk.endSec, chunk.embedding])
    try await db.execute("INSERT INTO chunks_fts(chunk_id, text) VALUES (?, ?)",
                         params: [chunk.id.uuidString, chunk.text])
}

func topK(query: String, scope: [UUID]?, k: Int, db: Database) async throws -> [Hit] {
    let qvec = try await embedder.embed([query])[0]
    let scopeClause = scope.map { "AND podcast_id IN (\($0.map { "'\($0)'" }.joined(separator: ",")))" } ?? ""
    let rows = try await db.query("""
        SELECT chunk_id, episode_id, start_sec, end_sec, distance
        FROM chunks_transcript
        WHERE embedding MATCH ? \(scopeClause)
        ORDER BY distance LIMIT ?
    """, params: [qvec, k])
    return rows.map(Hit.init)
}
```

---

## 9. Cost Model — Power User, 50h/week of Transcripts

- 50 hours of speech ≈ **450K spoken words** ≈ **600K tokens** (1.33 tok/word).
- Sliding chunks with 15% overlap → effective tokens to embed ≈ **690K/week**.
- At `text-embedding-3-large` $0.13/M → **~$0.090/week of ingest**, or **$4.66/year**.
- At `text-embedding-3-small` $0.02/M → **$0.014/week**, **$0.72/year**.
- Wiki regeneration (assume 200K tokens/week) adds ~$0.026/week at large.

**Query side:** assume 200 agent-tool calls/week with reranker.
- Embed query: 200 × 50 tok × $0.13/M = **$0.0013/week**.
- Rerank: 200 × $0.001 = **$0.20/week** = $10.4/year.

**Total power user: ~$15/year API cost.** Trivial. We could ship without BYOK and absorb it; or expose BYOK and pocket nothing. Either way, the cost is not a constraint — the constraint is latency and UX polish.

---

## Summary Recommendation Stack

- **Embeddings:** OpenRouter → `openai/text-embedding-3-large` @ 1024-d, abstracted behind `EmbeddingProvider`.
- **Reranker:** OpenRouter → `cohere/rerank-v3.5`, optional path.
- **Vector store:** `sqlite-vec` via `jkrukowski/SQLiteVec`, single `vectors.sqlite` file in the app group.
- **Search:** Hybrid FTS5 + vec0 + RRF, cosine metric, Matryoshka 1024-d.
- **Metadata system of record:** SwiftData (linked by `episodeID: UUID`).
- **Chunking:** sliding 400–512 tok / 15% overlap for transcripts; semantic for wikis.
- **Indexes:** one global transcript index, one global wiki index, scope-filtered at query time.
- **Refresh:** append-on-new-episode, replace-on-wiki-regen, full rebuild on model bump.

## Sources

- [OpenRouter Embeddings API reference](https://openrouter.ai/docs/api/reference/embeddings)
- [OpenRouter embedding models collection](https://openrouter.ai/collections/embedding-models)
- [OpenRouter `text-embedding-3-large` page](https://openrouter.ai/openai/text-embedding-3-large)
- [OpenRouter rate limits docs](https://openrouter.ai/docs/api/reference/limits)
- [OpenRouter BYOK guide](https://openrouter.ai/docs/guides/overview/auth/byok)
- [OpenAI new embedding models announcement](https://openai.com/index/new-embedding-models-and-api-updates/)
- [Voyage AI pricing](https://docs.voyageai.com/docs/pricing)
- [Cohere `rerank-v3.5` on OpenRouter](https://openrouter.ai/cohere/rerank-v3.5)
- [`sqlite-vec` v0.1.0 release notes](https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/index.html)
- [`sqlite-vec` on iOS guide](https://alexgarcia.xyz/sqlite-vec/android-ios.html)
- [Hybrid FTS5 + vec0 search post](https://alexgarcia.xyz/blog/2024/sqlite-vec-hybrid-search/index.html)
- [`SQLiteVec` Swift package](https://github.com/jkrukowski/SQLiteVec)
- [USearch repo](https://github.com/unum-cloud/usearch)
- [SwiftSemanticSearch USearch example](https://github.com/ashvardanian/SwiftSemanticSearch)
- [ObjectBox Swift on-device vector index](https://objectbox.io/swift-ios-on-device-vector-database-aka-semantic-index/)
- [Apple `NLEmbedding` docs](https://developer.apple.com/documentation/naturallanguage/nlembedding)
- [Stack Overflow: chunking in RAG](https://stackoverflow.blog/2024/12/27/breaking-up-is-hard-to-do-chunking-in-rag-applications/)
- [Firecrawl: best chunking strategies for RAG](https://www.firecrawl.dev/blog/best-chunking-strategies-rag)

---

File path: `/Users/pablofernandez/Work/podcast-player/.claude/research/embeddings-rag-stack.md`
