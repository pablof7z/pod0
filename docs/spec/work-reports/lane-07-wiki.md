# Lane 07 — LLM Wiki Generation Pipeline + Browser UI

> Branch: `worktree-agent-a7ed15a8c1c1ae440`
> Status: build green, all tests passing (33 / 33)

## What this lane shipped

Two concerns, one commit:

1. **Wiki engine** under `App/Sources/Knowledge/` — Codable model, file-system storage, generator pipeline with citation verification, declarative refresh triggers, and a stripped-down OpenRouter client tuned for the compile turn.
2. **Wiki browser UI** under `App/Sources/Features/Wiki/` — library home, paper-feel page renderer, Liquid Glass citation peek, "compile a page about X" sheet, and a deterministic mock fixture.

Plus a unit-test suite at `AppTests/Sources/WikiVerifyTests.swift` covering snippet clamp, slug normalization, every verifier branch, storage round-trip, trigger production, and the end-to-end stubbed-LLM compile path.

## Files added

### Knowledge

| File | Lines | Purpose |
|---|---|---|
| `WikiPage.swift` | 162 | Top-level page model + `WikiPageKind` + `WikiScope` |
| `WikiSection.swift` | 102 | Section + `WikiSectionKind` + claim model |
| `WikiCitation.swift` | 128 | Citation + `WikiConfidenceBand`, ≤125-char snippet clamp |
| `WikiStorage.swift` | 240 | Atomic JSON-on-disk persistence + inventory registry |
| `RAGSearchProtocol.swift` | 126 | Protocol Lane 6 satisfies + `InMemoryRAGSearch` for tests |
| `WikiOpenRouterClient.swift` | 145 | Live + stubbed OpenRouter wrapper, JSON-mode, no streaming |
| `WikiPrompts.swift` | 187 | System prompt + topic / person / show / audit templates |
| `WikiResponseParser.swift` | 162 | Liberal LLM-JSON → `WikiPage` decoder |
| `WikiVerifier.swift` | 231 | Post-compile pass: drops fabricated citations, demotes confidences |
| `WikiGenerator.swift` | 150 | Orchestrator (RAG → prompt → LLM → parse → verify → persist) |
| `WikiTriggers.swift` | 195 | Pure producer of `WikiRefreshJob`s from system events |

### Features/Wiki

| File | Lines | Purpose |
|---|---|---|
| `WikiView.swift` | 199 | Library home, scope picker, list of pages |
| `WikiHomeViewModel.swift` | 120 | `@Observable` model loading inventory + scope filter |
| `WikiPageView.swift` | 246 | Editorial paper-style page renderer |
| `CitationPeekView.swift` | 133 | Liquid Glass peek-from-below with play-clip stub |
| `WikiGenerateSheet.swift` | 214 | Compile-a-page flow, 2s "thinking" UI, returns fixture |
| `WikiMockFixture.swift` | 230 | Deterministic sample pages used by previews + empty home |

### Tests

| File | Lines | Purpose |
|---|---|---|
| `AppTests/Sources/WikiVerifyTests.swift` | 393 | 16 unit tests: clamp, slug, verify, storage, triggers, end-to-end |

All files are well under the soft 300-line cap except `WikiVerifyTests.swift` (test file; intentionally exhaustive) and `WikiPageView.swift` (246, under cap), `WikiStorage.swift` (240, under cap).

## Pipeline

```
WikiGenerator.compileTopic(topic, scope)
  ├─ RAGSearchProtocol.search(topic, scope, limit) → [RAGChunk]
  ├─ WikiPrompts.topic(...)                        → user prompt
  ├─ WikiOpenRouterClient.compile(system, user)    → JSON string
  ├─ WikiResponseParser.parse(json, ...)           → draft WikiPage
  └─ WikiVerifier.verify(draft)
       ├─ for each claim:
       │    ├─ if isGeneralKnowledge → keep, demote to .low
       │    ├─ if no citations → drop
       │    ├─ resolve every citation via RAG.chunk(episode, span)
       │    │    ├─ verbatim substring match → .high
       │    │    ├─ ≥60% token overlap     → .medium
       │    │    └─ else                   → .low
       │    └─ unresolved citations → demote claim band by one rung
       └─ recompute page-level confidence as 0.7·survival + 0.3·model self-rating
WikiStorage.write(page)  ← caller decides; not implicit
```

The verifier is independent of the LLM — it only talks to `RAGSearchProtocol`. That keeps it cheap (no second round-trip), deterministic in tests, and means a swap of the underlying RAG implementation can't change the verification semantics.

## Storage layout

```
Application Support/podcastr/wiki/
  _inventory.json
  global/
    ozempic.json
    mitochondrial-uncoupling.json
  podcast/
    <podcast-id>/
      <slug>.json
```

All writes are atomic via `Data.write(.atomic)`. The inventory is a separate file so the home can list 1k pages without decoding every body. Slugs are URL-safe-folded (`Mitochondrial Uncoupling!` → `mitochondrial-uncoupling`) and stable across renames.

## Triggers

`WikiTriggers` is a pure producer: feed it events, it returns refresh jobs. No scheduler. Four event kinds:

- `episodeTranscribed(...)` → fan out one job per *existing* page whose slug matches an extracted topic/person. Per-podcast pages outrank library pages in priority.
- `userContestedClaim(...)` → high-priority single-page job.
- `episodeRemoved(...)` → low-priority sweep across every page that has citations.
- `modelMigrated(...)` → low-priority sweep across the whole library.

The producer never *creates* pages from a trigger, only refreshes existing ones. Page creation is always user-invoked through the generate sheet.

## UI

- **WikiView** — library home. Paper background (`#F6F2E9` light / `#0E0F12` dark — UX-04 §4). List of pages with confidence-keyed left margin rule, citation count, relative date. Empty state surfaces a "compile a page" CTA. Scope picker is segmented Library / per-podcast.
- **WikiPageView** — editorial paper render. New York serif at 34pt for the title, italic body with 4pt line-spacing, hairline section dividers, confidence margin rule per claim, citation chips at amber `#B8741A`. Footer carries `rev N · model · relative date`.
- **CitationPeekView** — Liquid Glass sheet at `cornerRadius: 22`, `presentationDetents([.fraction(0.42), .medium])`. Header shows speaker + timestamp + verification confidence pill; body renders the verbatim quote in serif italic; primary action is "Play clip" which posts a `podcastr.wiki.playClip` `Notification.Name` for the player surface to observe.
- **WikiGenerateSheet** — staged compile animation (search → draft → resolve → done) totalling ~2 seconds, returns a fixture page modelled on `WikiMockFixture.ozempicTopic` with the requested title.
- **WikiMockFixture** — three sample pages (Ozempic topic, Mitochondrial Uncoupling topic, Huberman Lab show) with stable UUIDs.

## Constraints honoured

- No SPM deps added (zero `Package.swift` / `Project.swift` changes).
- File-size: every new file under the soft 300-line cap except `WikiPageView.swift` (246) and `WikiStorage.swift` (240) which sit comfortably below the hard 500. Test file is 393.
- Real LLM calls are stubbed: `WikiOpenRouterClient.stubbed(json:)` mode returns fixture JSON; `WikiGenerateSheet` runs a 2s mock animation and returns a `WikiMockFixture`-derived page.
- Build: `xcodebuild -workspace AppTemplate.xcworkspace -scheme AppTemplate ... build` → `BUILD SUCCEEDED`.
- Tests: 33/33 pass (17 pre-existing + 16 new).

## NOT touched

Confirmed untouched: `App/Sources/Audio/`, `App/Sources/Podcast/`, `App/Sources/Features/Library/`, `App/Sources/Features/Player/`, `App/Sources/Features/EpisodeDetail/`, `App/Sources/Transcript/`, `App/Sources/Voice/`, `App/Sources/Briefing/`, `App/Sources/Features/Briefings/`, `App/Sources/Agent/`, `App/Sources/Features/Agent/`, `Project.swift`, `App/Resources/Info.plist`. (Several of these directories don't yet exist in this worktree — sister lanes will create them.)

## Coordination notes for sister lanes

- **Lane 6 (RAG):** conform your concrete search type to `RAGSearchProtocol` (`App/Sources/Knowledge/RAGSearchProtocol.swift`). The protocol asks for two methods: `search(query:scope:limit:)` and `chunk(episodeID:startMS:endMS:)`. Lane 7 only depends on the protocol; replacing `InMemoryRAGSearch` is a one-line change at `WikiGenerator` construction.
- **Lane 4-5 (Player):** to wire the citation-peek "Play clip" button, observe `CitationPeekView.playClipNotification` (`Notification.Name("podcastr.wiki.playClip")`) and read `episodeID`, `startMS`, `endMS` from `userInfo`. We'll happily migrate to a typed deep-link router once one exists.
- **Lane 10 (Agent tools):** `query_wiki` should call `WikiStorage.list(scope:)` for inventory queries and `WikiStorage.read(slug:scope:)` for page bodies. Both are `Sendable`.
- **Root navigation:** `WikiView` is not yet wired into `RootView.swift`'s tab bar — that's a coordinator-level decision (presumably Lane 1 or a final integration pass). The view is preview-ready and renders standalone.

## Test summary

```
Test Suite 'WikiVerifyTests' passed at 2026-05-09 15:33:21.173.
  Executed 16 tests, with 0 failures (0 unexpected) in 0.133 (0.137) seconds
Test Suite 'AppTemplateTests.xctest' passed.
  Executed 33 tests, with 0 failures (0 unexpected) in 0.327 (0.337) seconds
** TEST SUCCEEDED **
```

Verifier coverage:
- Snippet clamp keeps short / truncates long.
- Slug normalization handles punctuation, whitespace, diacritics, edge case (no usable chars → "untitled").
- Verbatim citation match → `.high`.
- Unresolved citation → claim dropped.
- Unsourced claim → dropped.
- General-knowledge unsourced claim → kept, demoted to `.low`.
- Partially-resolved claim → demoted by one band.
- Page-level confidence blends survival rate + model self-rating.
- Fuzzy match hits on token overlap; misses on poor overlap.
- Storage round-trip preserves title + claim text + inventory entry.
- Storage delete clears both file and inventory.
- Triggers produce jobs only for existing slugs on new-episode events.
- Triggers flag every page on model migration.
- End-to-end: stubbed LLM JSON → parse → verify → persist → re-read.
