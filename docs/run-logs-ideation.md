# Agent Run Logs — Ideation

Cross-cut design doc for porting the **Run Logs** feature from
`win-the-day-app` (`RockingLife`) into podcast-player. Three Opus agents were
run in parallel — Architecture, Product/UX, and Implementation — and this doc
consolidates their findings into a decision-ready brief.

## What we're porting

A historical, drillable view of every agent run: input, system prompt,
turn-by-turn `messagesBeforeCall` snapshot, the assistant message, every tool
call with arguments and result, token usage, duration, outcome
(`completed` / `turnsExhausted` / `failed`), and `failureReason`.

Reference implementation in `win-the-day-app`:

- `RockingLife/Agent/AgentRunLog.swift` — data model + JSON-on-disk logger
  (`AgentRun`, `AgentRunTurnData`, `AgentAPIResponse`, `AgentToolCall`,
  `AgentToolDispatch`, `AgentTokenUsage`, `AgentRunOutcome`, `AnyCodable`)
- `RockingLife/Agent/AgentSession.swift` — turn-loop instrumentation pattern
  (collector → finalize on every exit branch)
- `RockingLife/Features/Settings/Agent/AgentRunListView.swift`,
  `AgentRunDetailView.swift`, `AgentRunToolFormatter.swift` — UI surface

## Where it lands in podcast-player

The single funnel for every agent run is
`App/Sources/Features/Agent/AgentChatSession.swift` `runAgentTurns(batchID:)`.
Voice piggybacks on this through `Voice/VoiceTurnDelegate.swift`. Briefings
invoke the LLM as a tool from inside the chat session — they do **not** open a
second turn loop. The only true second entry point is
`App/Sources/Agent/AgentRelayBridge.reply` (Nostr-inbound).

## Existing surfaces this must reconcile with

- `App/Sources/Features/Agent/AgentActivitySheet.swift` — **live** in-flight tool
  activity, keyed by `batchID`, intended for confidence + undo.
- `App/Sources/Features/Settings/Agent/AgentActivityLogView.swift` — historical
  *side-effects* log.
- `App/Sources/State/CostLedger.swift` — token/cost accounting.

Decision: **Run Logs is a separate, new feature.** It is the *cognitive trace*
(I/O of the LLM and tool dispatcher); the activity sheet stays the *side-effects
trace* (what the agent changed in your data). They link to each other but share
no state. CostLedger and Run Logs are joined by an optional `runID` rather than
duplicated.

---

## 1. Architecture

### Data model — port verbatim, with three adjustments

Bring over `AgentRun`, `AgentRunTurnData`, `AgentAPIResponse`, `AgentToolCall`,
`AgentToolDispatch`, `AgentTokenUsage`, `AgentRunOutcome`, `AnyCodable` from
the reference. Adjustments:

1. **Tool argument shape mismatch.** Reference parses to `[String: Any]`.
   Podcast-player's `AgentToolCall` (`AgentOpenRouterClient.swift:221-225`)
   already carries the **raw JSON string**. Parse to `[String: AnyCodable]`
   *inside* the logger boundary so the hot path stays string-only and on-disk
   shape stays inspectable.
2. **`tokensUsed` is missing on `AgentResult`** (`AgentOpenRouterClient.swift:227-230`)
   — it is captured into `CostLedger` instead. Two options:
   - (a) extend `AgentResult` with `tokensUsed: AgentTokenUsage?`
   - (b) add a `runID` to `UsageRecord` so Run Logs and Cost Ledger join.
   **Recommend (b)** — single source of truth, no duplication. (a) is fine for
   PR-1 if (b) is invasive.
3. **Domain enrichment** (Phase 2): optional `podcastContext` on the run —
   currently-playing `EpisodeID`, `PodcastID`, playback timestamp at start,
   active briefing handle, transcript-chunk count fed to the prompt. Voice
   runs additionally carry `transcriptionConfidence` and recogniser locale.

### `AgentRunSource` cases

Replace the reference's enum with one tagged at the **call site**, not by
content sniffing:

```
.typedChat           // AgentChatSession from chat UI
.voiceMessage        // AgentChatSession via VoiceTurnDelegate
.nostrInbound        // AgentRelayBridge.reply
.briefingCompose     // future: standalone briefing job, if it ever leaves the chat session
.background          // future
.manual              // dev/test
```

### Persistence — split decision

This is the only place the agents disagreed.

- **Architecture angle:** SQLite sidecar — runs are large (tens of KB each),
  the reference's full-array re-encode on every insert is O(n²), and
  `EpisodeSQLiteStore.swift` already establishes the SQLite pattern.
- **Implementation angle:** JSON-on-disk like the reference — append-mostly,
  read-rarely (only when the user opens Run Logs), and a 200-run cap keeps the
  file small.

**Recommendation:** Start with **Application Support JSON** (impl angle)
because parity with the reference de-risks PR-1, and any voice-mode write
amplification problem will surface fast. If runs grow beyond a few MB or write
hitches show up during voice playback, escalate to **one-file-per-run + an
index**, then SQLite only if needed. Don't pre-emptively introduce SwiftData;
the rest of the app is hand-rolled SQLite + Codable, and mixing ORMs invites
schema drift.

### Concurrency

Both `AgentChatSession` and `AgentRelayBridge` are `@MainActor`, as is the
reference logger. Keep `AgentRunLogger` `@MainActor`/`ObservableObject` for the
UI-facing array. If/when persistence escalates to SQLite, hide the I/O behind
a non-`MainActor` `actor` and let the logger forward writes via a non-blocking
`Task`. The collector mutates only on the main actor; the cross-actor object
is the single finalised `AgentRun` value (Sendable).

### Privacy / size

- **Per-run cap:** ~256 KB encoded; truncate the longest `messagesBeforeCall`
  blob first (it grows monotonically and is the most redundant).
- **Per-message content cap:** ~16 KB before persist, to prevent a single
  long-transcript turn blowing the budget.
- **Retention:** 100–200 runs OR 30 days, whichever first.
- **Redaction:** strip Authorization headers (none today, future-proof);
  per-tool `redactionPolicy` for transcript-heavy tools (`truncate(2KB)` for
  transcript queries, `passthrough` for IDs); `AgentRun.redact()` pass before
  any export/share path.
- **Export only on user action.** Never auto-upload.

---

## 2. Product / UX

### Naming & entry point

- **Settings row label:** "Run History"
- **Screen title:** "Agent Runs"
- **Lives in:** Settings → Agent, directly above the existing "Activity Log"
  row in `App/Sources/Features/Settings/Agent/AgentSettingsView.swift:84`.
- Icon: `clock.badge.questionmark` or `doc.text.magnifyingglass`.
- Badge: count of failed runs in last 24h (hidden when zero).
- **Voice mode hook:** small `info.circle` in the voice caption that
  deep-links to the just-finished run. **No** voice-mode tab.
- "Logs" reads dev-only; "History" invites the curious user.

### List row — four lines, podcast-aware

```
┌──────────────────────────────────────────────────────────┐
│ 🎙  Voice  · Now playing                       2:47 PM   │
│ "play that bit about zone 2 from huberman"               │
│ ▶ Huberman Lab · E142 · jumped to 41:08                  │
│ ↻ 3 turns   ƒ 2.1k tokens   ⏱ 4.2s            ✓ done    │
└──────────────────────────────────────────────────────────┘
```

The third line — *what the agent actually did, in user terms* — is the
podcast-specific addition. Resolve episode UUIDs to titles, timestamps to
`mm:ss`. Reference's row is three lines; we add this fourth.

**Filters** (chips, top): All · Voice · Typed · Briefings · Failed ·
Has tool calls. Reuse chip styling from the existing Activity Log.

**Sectioning:** bucket by `RelativeDateBucket` (already in the codebase) —
Today / Yesterday / Last 7 days. Voice mode produces runs faster than
win-the-day's flat list can scan; grouping is non-negotiable.

**Empty state:** `ContentUnavailableView`, `waveform.badge.magnifyingglass`,
copy: *"No runs yet. Ask the agent something — by voice, by text, or from a
briefing — and you'll see exactly how it answered here."*

### Detail view — section order

1. **Header** — source, started, duration, turns, tokens, outcome. Add
   *trigger surface*: "Chat", "Voice mode", "Briefing: This week",
   "Episode: Huberman E142 player".
2. **Context (NEW, podcast-specific):** collapsible "When this run started"
   card — active episode artwork + title, playback position, sleep timer,
   playback rate, briefing handle, transcript chunk count.
3. **Failure** (if any) — red header, full `failureReason`.
4. **Tools used** — flat list, podcast-aware formatter (next section).
5. **Inspection** — System Prompt, Messages.
6. **Per-turn** — assistant text + token deltas (reference parity).

```
┌─ When this run started ──────────────────┐
│ [art] Huberman Lab — E142 "Sleep & Light" │
│       Playing · 38:12 / 1:47:33 · 1.5x   │
│ Sleep timer: off                          │
│ Transcript chunks fed in: 6  (tap →)     │
└───────────────────────────────────────────┘
```

### Tool call presentation — podcast-aware formatter

Generic `AgentRunToolFormatter` works on shape, not semantics. Add a
parallel `PodcastToolFormatter` that resolves IDs at render time:

| Tool | Generic | Podcast-aware |
|---|---|---|
| `play_episode_at` | `play_episode_at` | **Played episode at 41:08** — Huberman Lab — E142 *Sleep & Light* |
| `query_transcripts` | `query_transcripts` | **Searched transcripts** — `"zone 2 training" · 6 chunks · scope: Huberman` |
| `generate_briefing` | `generate_briefing` | **Generated briefing** — This week · 10 min · deep_dive |
| `set_playback_rate` | `set_playback_rate` | **Changed speed to 1.5x** |

**Never show raw UUIDs in the human surface.** Tap a row to reveal the raw
arguments JSON underneath.

### Reconciling with `AgentActivitySheet`

Two different objects, kept separate, linked bidirectionally:

- **AgentActivitySheet footer:** "View the full agent run →"
  `AgentRunDetailView`
- **Run Detail → Tools used (rows that mutate state):**
  `arrow.uturn.backward` chip → `AgentActivitySheet(batchID:)`

Do **not** morph the live sheet into "current run" — the live sheet's job is
in-the-moment confidence + undo; replacing it with a verbose run trace breaks
the calm UX during a voice conversation.

### Affordances that earn their slot

- Keep: **Copy full JSON**, **Copy system prompt**, **Share**.
- Add: **Copy curl repro** (the exact OpenRouter call that produced turn N).
- Add: **Retry this run** (failed runs only) — re-injects the original input
  into a fresh chat session. Highest-value debug affordance.
- Add: **Report a bug** — pre-fills a sheet with run ID + redacted JSON.
- Add: list-row leading swipe = **Star** (surfaces under a "Starred" filter
  chip — users *will* return to a great answer).
- Drop: Pin, Mark interesting (semantically thin, no retrieval surface).

### Edge cases

- **Voice runs (long):** detail view must lazy-load turn cards. Add a
  turn-strip jump bar at top: `1 ▪ 2 ▪ 3 ▪ 4 ✗ 5` with per-turn outcome dots.
- **Failed runs:** red header, failure section first, **Retry this run**.
- **No tool calls:** hide the "Tools used" card entirely — never show
  "0 tools".
- **Privacy on share:** voice runs may capture ambient transcript. Every
  Share / Copy JSON path must offer a "Redact transcript" toggle.

---

## 3. Implementation plan

### File-by-file change set

**NEW**

| File | Purpose |
|---|---|
| `App/Sources/Agent/RunLog/AgentRunLog.swift` | Data types + `AnyCodable`. ≤ 300 lines (per `AGENTS.md` cap). |
| `App/Sources/Agent/RunLog/AgentRunLogger.swift` | `@MainActor` `ObservableObject` singleton, JSON persistence to Application Support. |
| `App/Sources/Agent/RunLog/AgentRunCollector.swift` | Mutable per-run accumulator used inside `runAgentTurns`. |
| `App/Sources/Features/Settings/Agent/AgentRunListView.swift` | Run list. |
| `App/Sources/Features/Settings/Agent/AgentRunDetailView.swift` | Per-run drilldown. Reference is 566 lines; **split** at the 500-line cap into shell + `AgentRunTurnSection.swift` + `AgentRunMessageRow.swift`. |
| `App/Sources/Features/Settings/Agent/AgentRunToolFormatter.swift` | Phase 1: generic. Phase 2: podcast-aware overrides. |

**EDIT**

| File | Change |
|---|---|
| `App/Sources/Features/Agent/AgentChatSession.swift` | Instrument `runAgentTurns(batchID:)` — see capture map below. Thread `source: AgentRunSource` through `send`/`startSend`/`regenerateSend`. |
| `App/Sources/Voice/VoiceTurnDelegate.swift` | Pass `.voiceMessage` into `startSend`. |
| `App/Sources/Agent/AgentRelayBridge.swift` | Wrap `reply`'s `for _ in 0..<maxTurns` loop with the same collector pattern, `source: .nostrInbound`. |
| `App/Sources/Features/Agent/AgentLLMClient.swift` (+ `AgentOpenRouterClient.swift`, `AgentOllamaClient.swift`) | Extend `AgentResult` with `tokensUsed: AgentTokenUsage?`. Best-effort — pass `nil` if the provider doesn't surface usage. |
| `App/Sources/State/CostLedger.swift` (Phase 2) | Add `runID: UUID?` to `UsageRecord` so Run Logs and Cost Ledger join. |
| `App/Sources/Features/Settings/Agent/AgentSettingsView.swift` | Add a `NavigationLink` row pointing at `AgentRunListView()` after the existing Activity Log row (~line 84). |

### Capture instrumentation in `AgentChatSession.runAgentTurns(batchID:)`

Single funnel, line numbers from the current 377-line file:

| Where | What |
|---|---|
| Line 223 (entry) | record `startTime`, snapshot `initialInput` and `systemPrompt`, instantiate `AgentRunCollector`. |
| Line 232 (before `streamCompletion`) | snapshot `messagesBeforeCall = rawMessages`. |
| Line 238 (catch `CancellationError`) | `collector.finish(outcome: .cancelled, ...)` — recommend adding a new `.cancelled` case so voice barge-ins don't vanish. |
| Line 245 (catch error) | append a partial turn (apiResponse=nil, dispatches=[]), `collector.finish(outcome: .failed, failureReason: error.localizedDescription)`. |
| Line 259 (after `rawMessages.append(result.assistantMessage)`) | build `AgentAPIResponse`, stash on collector's pending turn. |
| Lines 266–271 (no tool calls, return) | `collector.appendTurn(...)`, `collector.finish(outcome: .completed)`. |
| Lines 287–293 (after `AgentTools.dispatch`) | append `AgentToolDispatch` with parsed args + result + any error. |
| Line 300 (end of loop body) | flush pending turn. |
| Line 326 (turns exhausted) | `collector.finish(outcome: .turnsExhausted)`. |

Build the `AgentToolDispatch` at the **call site**, not inside
`AgentTools.dispatch` — the dispatcher is shared with the relay bridge and
shouldn't know about run logs. Parse the result JSON once and reuse for both
the `rawMessages.append` payload and the dispatch record.

### PR-1 checklist

1. Create `App/Sources/Agent/RunLog/`. Add `AgentRunLog.swift` (types +
   `AnyCodable`) and `AgentRunLogger.swift` (singleton + persistence). Each
   file < 300 lines.
2. Extend `AgentResult` in `AgentLLMClient.swift` with
   `tokensUsed: AgentTokenUsage?`. Propagate in both client impls
   (best-effort).
3. Add `AgentRunCollector` and instrument every exit path in
   `AgentChatSession.runAgentTurns` per the table above. Thread `source`
   through `startSend` / `send` / `regenerateSend`. Voice delegate passes
   `.voiceMessage`.
4. Wrap `AgentRelayBridge.reply`'s loop with the same collector,
   `source: .nostrInbound`.
5. Port `AgentRunListView`, `AgentRunDetailView` (split if > 500 lines),
   `AgentRunToolFormatter` into `App/Sources/Features/Settings/Agent/`. Wire
   to `AgentRunLogger.shared` via `@StateObject` / `@ObservedObject`.
6. Add the "Run History" `NavigationLink` row in
   `AgentSettingsView.agentSection`, badge = `AgentRunLogger.shared.runs.count`.
7. Manually verify all four sources end-to-end: typed → `.typedChat`,
   voice → `.voiceMessage`, Nostr DM (or stub) → `.nostrInbound`, force
   network error → `.failed` + `failureReason`, max-turns → `.turnsExhausted`.
8. `swift build` clean, no Sendability warnings on `AgentRun` /
   `AgentRunTurnData`. Open Settings → Agent → Run History in the simulator
   and confirm a real run renders with turn-by-turn detail.

### Phased rollout

- **Phase 1 — parity port (PR-1):** Types, logger, list, detail, formatter,
  settings link, instrumentation in `AgentChatSession` + `AgentRelayBridge`.
  Generic UI only. Token capture optional.
- **Phase 2 — podcast enrichments:** `episodeContext`, `playbackPositionSec`,
  `briefingID` on `AgentRun`. Podcast-aware tool formatter (UUID → title,
  seconds → `mm:ss`). "When this run started" context card. Bidirectional
  link with `AgentActivitySheet`. `runID` on `UsageRecord`.
- **Phase 3 — polish:** filters, sectioning by date bucket, share-as-JSON,
  retention cap (200 runs), redact-transcript toggle, "Retry this run",
  starred runs, "Copy curl repro".

### Risks & gotchas

- **Sendability:** `messagesBeforeCall` swallows `[String: Any]` via
  `AnyCodable` — proven in the reference; don't try to model the OpenAI
  message variants.
- **Payload size:** 20-turn voice runs with long transcript chunks can hit
  megabytes. Truncate per-message content at ~16 KB at persist time.
- **API-key redaction:** `AgentPrompt.build(for:)` shouldn't embed credentials
  but verify; scrub any `Authorization` headers that may echo through tool
  results. Add `AgentRun.redact()` before encoding.
- **MainActor:** all instrumentation lives on `@MainActor`; no hops needed.
  If a future headless scheduler enters here off-main, gate `log(run:)` with
  `await MainActor.run`.
- **Race with `AgentActivitySheet`:** they share no state. Keep it that way.
  Don't back the activity chip with run-log data.
- **Cancellation semantics:** `cancelSend()` discards partial content today.
  Decide explicitly — adding `.cancelled` to `AgentRunOutcome` is recommended
  so voice barge-ins are debuggable.
- **Regenerate path:** `regenerateLast` rewinds `rawMessages`. Create the
  collector fresh inside `runAgentTurns`; don't hold across calls or you'll
  log stale turns from discarded messages.

---

## Decisions to lock before PR-1

1. **Persistence:** JSON-on-disk (Application Support) for parity, escalate
   later if size/perf forces it. ✅ recommended.
2. **`tokensUsed` source:** extend `AgentResult` for PR-1; add `runID` to
   `UsageRecord` in Phase 2.
3. **`AgentRunOutcome`:** add `.cancelled` in PR-1.
4. **Settings entry point label:** "Run History" (user-visible) → screen
   title "Agent Runs".
5. **Phase 2 boundary:** podcast-aware tool formatter and `episodeContext` are
   Phase 2 — PR-1 ships generic.

## Open questions

- Should every `AgentChatSession.regenerateLast` discard the original failed
  run or keep it? Leaning **keep, mark linked** so the user can see what they
  retried.
- Voice barge-in: each `submitUtterance` is one run, or is a multi-utterance
  conversation one run? Recommend per-utterance (matches the reference's
  "one prompt → one run" intuition).
- Nostr-inbound runs may carry peer pubkeys that are pseudo-PII. Hash on
  export?
- Briefings *can* be authored standalone (no chat session). If/when that
  ships, it becomes the third entry point and needs its own collector wrap.

---

## Source angles

- Architecture: `Architect` (opus) — data model, persistence, capture points,
  concurrency, privacy.
- UX: `Designer` (opus) — IA, list row, detail section order, reconciling
  with `AgentActivitySheet`, podcast-aware tool presentation, edge cases.
- Implementation: `Engineer` (opus) — file-by-file change set, exact
  instrumentation lines in `AgentChatSession.runAgentTurns`, phased rollout,
  PR-1 checklist.
