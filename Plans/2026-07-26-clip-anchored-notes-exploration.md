# Clip-anchored notes exploration

Date: 2026-07-26
Project/context: pod0 — `Note`/`Anchor` (Swift) + `NoteTarget` (pod0-domain), clips, agent tool surface
Status: archived — shipped as PR #171 (schema 33, `NoteTarget::Clip`) on 2026-07-27

Kept for the reasoning rather than the conclusion: the conclusion is three lines, and most of the value is in which confident claims turned out to be wrong and how they were caught.

## Core Question

- How should a note be able to target a clip, given that `NoteTarget` today only knows `Note` and `Episode { position }`?
- Secondary: should the agent be able to create user-facing notes anchored to a clip, when its `create_note` tool currently takes only `text`?

## Current Working Model

- Notes and clips are two independent projections over the same episode timeline. Notes anchor to a *point* (`Anchor.episode(id, positionSeconds)`); clips are a *span* (`startMs...endMs`) with frozen `transcriptText` and an optional, never-user-editable `caption`.
- Nothing links them. There is no clip variant in `NoteTarget`, no query for "notes inside a clip's span", and no UI showing notes and clips together beyond per-episode counts in `StarredSegment`.
- Adding `NoteTarget::Clip { clip_id }` looks structurally cheap — the codec is already a tagged union with a per-variant column layout and an `Unsupported { wire_code }` escape hatch — but it is a real schema migration crossing domain → storage → facade → uniffi → Swift.

## Observations

- `NoteTarget` variants (`rust/crates/pod0-domain/src/notes.rs:38-49`): `Note { note_id }`, `Episode { episode_id, position_milliseconds }`, `Unsupported { wire_code }`. No clip variant.
- Swift `Anchor` mirrors this with only `.note(id:)` and `.episode(id:positionSeconds:)` (`App/Sources/Domain/Anchor.swift`).
- Storage codec is tagged with per-variant columns (`rust/crates/pod0-storage/src/note_store_codec.rs:60-127`): code 0=none, 1=note, 2=episode, 255=unsupported, plus `note_id` / `episode_id` / `position_ms` payload columns.
- The notes table has an **exact** required-column assertion (`rust/crates/pod0-storage/src/schema_notes.rs:30-52`) listing `target_code`, `target_note_id`, `target_wire_code` — there is no `target_episode_id`; the episode target reuses the shared `episode_id` / `position_ms` columns.
- Schema validation is a hard equality gate on the kernel version (`rust/crates/pod0-storage/src/schema.rs:38-49`), with versioned `require_columns` blocks layered by version (`if version >= 2`, `>= 3`, ...).
- A real migration framework exists: `pod0_migration_journal` and `pod0_backup_evidence` tables, `migration.rs`, `migration_db.rs`, extensive migration tests. Adding a column is a trodden path, not a wipe.
- Agent tool `create_note` / short name `n` takes **only** `text` (`rust/crates/pod0-application/src/agent_contract.rs:104-106`; catalog description "Save a note or reflection for the user." at `agent_tool_catalog.rs:57-62`).
- The agent's execution path hardcodes `None` for the target (`rust/crates/pod0-facade/src/runtime_agent_internal.rs:97-112`) — agent notes are currently unanchored to anything, not just to clips.
- ~~`create_clip` exists as an agent tool, so the agent can make a clip but not annotate one.~~ **CORRECTED 2026-07-26**: `create_clip` is wired but **never exposed**. It has a name mapping (`agent_tool_names.rs:97`), a contract variant (`agent_contract.rs`), and a full executor arm (`runtime_agent_internal.rs:141-161`) — but **no arm in `agent_tool_definition`**, which falls through to `_ => return None` (`agent_tool_catalog.rs:193`), and it is absent from `PRODUCT_PROOF_AGENT_TOOLS` (`agent_tool_catalog.rs:26-38`), which is the only list fed to the agent (`runtime_agent_commands.rs`). The agent cannot cut a clip *or* annotate one. `CreateNote` **is** in that list, so agent notes are real.
- **Caption has zero production writers — verified.** Every `caption:` literal in the app is in a `#Preview`: `SavedView.swift:83` and `ClippingsCard.swift:258`, both "On metabolism". No production Swift call site passes a caption (`PlayerShareSheet`, `AutoSnipController`, and the transcript composer all omit it), and the one path that *could* — agent `create_clip` — is unreachable per the correction above. So `Clip.caption` is always `nil`, and the `ClippingsCard:108-110` eyebrow never renders for any real user. A field with no writer and a reader that never fires.
- **But the kernel supports caption end to end**: `runtime_commands.rs:230-267` threads it through *both* `create_clip` and `update_clip`. So "redefine caption as `title` + give it entry UI" is a **pure Swift-side change with zero migration** (Cliff's finding, confirmed by reading the same span).
- Agent note writes are classed `reversible_write` / `durable_turn_grant` / decision owner `pod0_rust` (`docs/architecture/agent-tool-permissions.json`).
- User-facing note entry points today: player "Add Note" sheet anchored at the playhead (`App/Sources/Features/Player/PlayerView.swift:78-94`), voice-note sheet routed through the agent (`VoiceNoteRecordingSheet.swift:306` sets `pendingVoiceNoteAgentContext`), untargeted notes in Settings → Agent Notes (`AgentNotesView.swift:85`).
- Note display today: chapter-rail interleave via `ChapterRailItem` (`PlayerChaptersScrollView+Notes.swift:8-28`), the searchable list in `AgentNotesView`, per-episode counts in `StarredSegment.swift:65-88`, Spotlight, data export.
- **Forward-compat hazard**: `NoteRecord.swiftValue` returns `nil` when the target maps to `.unsupported` (`App/Sources/Core/SharedNoteMapping.swift:14-21, 95-110`), and `nil` drops the note from the projection entirely. An older build reading a clip-targeted note would silently lose the whole note, not just the anchor.
- **Coordination**: `@cliff-claude` is concurrently "Brainstorming the clip-as-artifact viewer (no code yet)" in `/pod0`. Directly adjacent — a clip viewer is the most likely host surface for clip-targeted notes. Positions exchanged 2026-07-26, recorded below.
- **VERIFIED (resolves the A-vs-B cost question)**: the notes table's target payload columns are `target_code`, `target_wire_code`, `target_note_id`, and the *unprefixed* shared `episode_id` / `position_ms` (`rust/crates/pod0-storage/src/library_store_notes.rs:53-54, 68-69, 124, 138-139`). Option B (`Clip { clip_id, position_ms }`) reuses the existing `position_ms` column, so **B costs exactly the same migration as A**: one new clip-id column. Cliff's cost-saver hypothesis is correct.
- **CORRECTION to a premise in Cliff's argument**: `ClipImageCardView` does **not** render `caption`. It renders showName, episodeTitle, the pull quote, speaker name, timestamp, and deep link only (`ClipImageCardView.swift:45-85`). Caption renders only as the in-app eyebrow in `ClippingsCard.swift:108-110`. The doc comment on `Clip.caption` *intends* it as a share headline ("User-editable headline shown above the prose on rendered shares", `Domain/Clip.swift:30`) but no share surface consumes it today.
- `NoteAuthor` in the domain is `User | Agent | Unsupported { wire_code }` (`rust/crates/pod0-domain/src/notes.rs:31-35`) — no pubkey dimension. A note authored by another identity is a `NoteAuthor` problem, **not** a `NoteTarget` problem. The two axes are orthogonal, so foreign-authored clip notes do not imply a fourth `NoteTarget` variant.

- **Agent tool surface, counted (2026-07-26)**: 53 `AgentToolName` variants; 42 have wire-name mappings (parseable); **12** have catalog definitions; **11** advertised. Defined-but-unadvertised = **exactly one: `RecordMemory`**. So the dominant pattern is *wired-but-never-defined* (~30 tools, `create_clip` among them), not deliberate withholding. Cliff's curation inference rests on n=1.
- `MAX_AGENT_TOOLS_PER_TURN = 46` (`agent_contract.rs:18`) against 11 advertised — capacity is not the constraint.
- The only list fed to the agent is literally named `PRODUCT_PROOF_AGENT_TOOLS`, and there is an active "iOS product-proof epic #56" in the workspace. Strong hint the list is scoped to a validation milestone rather than being a permanent capability gate. **Verify with Pablo** — this reframes "why isn't X exposed" for every tool, not just clips.

## Constraints And Invariants

- **User (2026-07-26): clips must absolutely be supported as the target of a note.** Non-negotiable.
- Rust owns note identity, revision, and persistence; Swift holds a projected presentation value only (`Domain/Note.swift:11-13`, `Domain/Clip.swift:5-6` — "never as a second durable writer").
- Clips freeze `transcriptText` at creation; re-ingest must never rewrite a saved excerpt. Any clip-note linkage must not reintroduce retargeting on transcript rebuild.
- `NoteEvidence` / `ClipEvidence` provenance is immutable once captured.
- Cross-language change ⇒ regenerated uniffi bindings and reconciled version literals (memory: pod0 cross-language merge resolution).
- AGENTS.md: user-facing change ⇒ `App/Resources/whats-new.json` entry; 300-line soft / 500-line hard file limits.

## Preferences

- (none stated yet beyond the hard constraint above)

## Assumptions

- Clips have a stable `ClipId` in the domain layer usable as a foreign key. **Verify**: check `pod0-domain` clip identity type and whether `pod0-storage` clip rows are addressable by it.
- Clip deletion is soft (`Clip.deleted`), so a note pointing at a deleted clip can degrade rather than dangle. **Verify**: confirm no hard-delete path in `clip_store*`.
- The kernel schema version can be bumped with an additive column migration without a store rebuild. **Verify**: read `migration.rs` for the additive-column pattern used by a prior version bump.

## Open Questions

- **Anchor shape**: does a clip target replace the episode anchor, or is the clip an *additional* dimension (note carries clip_id AND retains an episode position)? Drives whether the chapter rail can still place the note on the timeline.
- **One target or many**: `NoteTarget` is a single `Option<NoteTarget>`. Should a note ever target both a clip and an episode position? An enum forces exclusivity; a struct-with-optional-fields does not.
- **Derived vs explicit**: notes falling inside `clip.startMs...endMs` are *already* computable today with zero schema change. Is the requirement about explicit ownership ("this note belongs to this clip") or about display ("show me notes around this clip")? Different designs; the stated constraint points at explicit, worth confirming.
- **Where does the user create a clip-targeted note?** There is no clip detail screen today — clips are rows in `ClipsSegment` / cards in `ClippingsCard`. Implies a new surface (possibly @cliff-claude's clip-as-artifact viewer).
- **Does `caption` collapse into notes?** `Clip.caption` is a per-clip user-facing string with no entry UI. A `.free` clip-targeted note overlaps it. Two ways to write text onto a clip is a smell.
- **Agent surface**: optional target on `create_note`, or a distinct `annotate_clip` tool? Optional-target is smaller but widens an existing `durable_turn_grant` write.
- **Old-build behavior**: is silently dropping clip-targeted notes on older builds acceptable, or must `SharedNoteMapping` degrade unknown targets to a note-without-anchor *before* the new variant ships?

## Hypotheses

- H1 (unproven): `NoteTarget::Clip { clip_id }` = code 3 + one new `target_clip_id` column + version bump + `schema_notes.rs` column-list update + codec arms + uniffi regen + `Anchor` case + `SharedNoteMapping` arms. Mechanical, ~8 touch points, no data backfill.
- H2 (unproven): shipping the `SharedNoteMapping` degrade-don't-drop fix as a **separate earlier change** meaningfully reduces blast radius for users mid-upgrade. Only helps if users can run a build older than the variant — needs a read on the actual install/upgrade story.
- ~~H3: display-first as a pure query validates the surface before the schema moves.~~ **Retracted** — see Rejected Options. Cliff's refutation holds: zero notes target clips today because there is no way to author one, so a display-only clip page shows an empty margin for every clip in the library. It validates nothing.
- H4 (from Cliff, unproven): the right first ship is the clip **page** with bounded playback + boundary drag handles — zero schema change — with the margin landing on it after. De-risks the migration by proving the surface first. Distinct from H3 because it tests *the page*, not *the notes query*.

## Risks

- Schema version gate is hard equality — a botched migration is `CorruptSchema` on launch, i.e. the app fails to open the store. Recent history (`9fae0bdb` "Stop agent approvals from bricking the app") shows this class of cross-language mismatch already bricked a device build once.
- uniffi enum layout change is exactly the `3f80a934` failure mode: Swift and Rust must be rebuilt together or the FFI reads garbage. Adding a `NoteTarget` variant changes the enum's wire encoding.
- Old builds silently dropping clip-targeted notes reads to a user as data loss.
- Overlap between `Clip.caption` and a clip-targeted note could produce two competing "the text on this clip" concepts needing later reconciliation.
- `NoteTarget` becoming a general-purpose foreign key (clip, then chapter, then speaker, then podcast...) — each addition repeats the full migration. Worth deciding the growth story now rather than per-variant.
- Design collision with @cliff-claude's concurrent clip-viewer brainstorm if the two are not reconciled.

## Evidence Gathered

- Full read of `note_store_codec.rs`, `schema_notes.rs` column assertion, `schema.rs` version gate, `notes.rs` `NoteTarget`, `Anchor.swift`, `SharedNoteMapping.swift`, `agent_contract.rs`, `agent_tool_catalog.rs`, `runtime_agent_internal.rs` agent commit path, `PlayerView.swift` note sheet, `PlayerChaptersScrollView+Notes.swift`, `ClipsSegment.swift`, `StarredSegment.swift`.
- Grep confirms zero call sites querying notes by clip or clips by note.

## Adjacent Checks

- Adjacent check: how expensive is adding a `NoteTarget` variant across the stack?
  Finding: The codec is an explicitly-tagged union with a reserved `255 = Unsupported { wire_code }` slot and per-variant payload columns, sitting on a real migration framework (journal + backup evidence + versioned `require_columns`). The design anticipated new variants.
  Implication: Additive `NoteTarget::Clip { clip_id }` = code 3 runs with the grain of the existing design. Cost is migration discipline, not architecture.
  Confidence: high
  Caveat: run read-only in the main thread (background agents not dispatched this session per environment directive), so this is a source review, not an independent second opinion.

## Alternatives Considered

- **A. `NoteTarget::Clip { clip_id }`**: matches the existing grain; exclusive with the episode anchor, so a clip note loses its timeline position unless the rail derives one from the clip's `startMs`. Requires migration.
- **B. `NoteTarget::Clip { clip_id, position_milliseconds }`**: keeps the note placeable on the chapter rail *and* owned by the clip. Slightly more payload, same migration cost. Redundant if position is always derivable from the clip.
- **C. Derived-only (no schema change)**: query notes whose episode position falls inside `clip.startMs...endMs`. Ships today, zero migration, data already supports it — but no explicit ownership, so a note cannot survive its clip being retimed, and the user's constraint says targeting must be real.
- **D. Clip-side link (`Clip.noteIDs`, or reuse `caption`)**: puts the link on the clip. Avoids touching `NoteTarget`, but inverts the ownership direction used everywhere else and needs its own clip-table migration anyway.

## Peer Positions (@cliff-claude, viewer side, 2026-07-26)

- **Anchor**: option B with position **optional** — `Clip { clip_id, position_ms: Option<u64> }`. Rationale: (a) a note about one sentence inside a long clip needs its own point; (b) the viewer gives users drag handles on clip start/end, so a note pinned to `clip.startMs` *teleports* when the clip is retimed — a note carrying its own position stays put. Optional because a whole-artifact note ("this changed my mind") has no point and forcing one invents precision. **This is the annotation-vs-label distinction.**
- **Caption**: does *not* collapse into notes. Rename to `title`, give it entry UI. Different privacy classes; settling test — "if you share this clip, does this text go with it?" Caption always, note only on deliberate publish. Concedes the smell; fix is that only *one* of the two is presented as writing prose (caption = short label with LLM-suggested default for unlabeled auto-snips; notes = the only place you write thoughts).
- **Surface**: the clip detail page is Cliff's and is the host. A pushed **page**, not a sheet (clips already have deep links, so they already have URLs). Three zones: bounded playback of just this span with drag handles → **the margin** (notes, layered over time, empty state as invitation not chore) → relationships (source episode, collections, chats it's been attached to, later others' clips of the same moment). Key claim: **the margin is the page's primary content; the clip is the letterhead.**
- **+1 on H2** independent of everything else: losing an anchor should never lose the note.
- **`create_clip` exposure — both agents recommend NO for now.** Cliff's design reason (agreed): a clip the agent cut on its own initiative is a *recommendation*, not an intersection between the work and the person; it dilutes a list whose entire value is that everything in it was chosen. `Clip.Source` has `.agent` and `ClippingsCard` badges non-touch sources, but badging doesn't restore chosenness. The right version is **agent-proposed / user-committed** — a candidate span the user can play, nudge with the same boundary handles, then keep or discard; it becomes a `Clip` only on commit. That is a proposal surface, belongs with the Ask verb, and shipping the fire-and-forget write first forecloses it.
- Cliff withdrew the "deliberate curation" premise after the denominator count (~41 wired-but-never-defined vs 1 defined-but-withheld) and routed the milestone-scoping question to `@meadow-codex`, who owns epic #56. Removes a fifth item from Pablo's queue.

## Rejected Options

- **C (derived-only) as a first ship / validation step**: refuted by Cliff. No notes can target clips today, so the derived query renders an empty margin everywhere. Still valid as an *additional* display affordance later ("notes near this clip's span"), never as the targeting mechanism.
- **Rename-only / collapse `caption` into notes**: deferred rather than rejected — the privacy-class argument is sound in intent even though its cited evidence (caption on `ClipImageCardView`) is factually wrong today.

## Decisions Or Emerging Direction

- **Constraint (Pablo, explicit)**: clips must be a first-class `NoteTarget`.
- ~~Emerging: `Clip { clip_id, position_milliseconds: Option<u64> }`, converged between both agents.~~ **REVERSED 2026-07-26 by Pablo.** Two objections, both correct:
  - *"Doesn't a clip imply a position already?"* It does. And a note about a specific sentence is **already served by the existing `Episode { episode_id, position_milliseconds }` variant** — that *is* a moment-anchored note. Neither agent noticed we were duplicating an existing capability. Clean split: **moment → `Episode`, artifact → `Clip { clip_id }`**.
  - Cliff's retiming case inverts on inspection: a note carrying a position inside a clip whose handles are then dragged becomes a note pointing at a clip while positioned *outside* it. The optional-position enum **permits an invalid state** requiring reconciliation logic. Under the narrow shape, retiming needs no machinery — a moment-note outside the narrowed span simply stops appearing on the clip, which is what narrowing means.
  - Deferring is cheap: `position_ms` is already a shared column on the notes table, so adding position to the clip variant later is an FFI/enum change with **no migration**. Asymmetry favors starting narrow.
- **DECIDED**: `NoteTarget::Clip { clip_id: ClipId }`, `target_code = 3`, one new column on the **notes** table. Decision signal: Pablo's objection to me ("doesn't a clip imply a position already?") plus his framing relayed via Cliff — *"a note is attached to a clip the same way a margin note is attached to a highlight in a book."* A margin note attaches to the **highlight**, not to a point inside it. Two independent signals, same direction.
- **The split makes invalid states unrepresentable** (Cliff's articulation, agreed): a clip note has no position, so it can never sit outside its clip; a moment note has no clip, so it can never contradict one. Neither variant can reach an inconsistent state. This only became visible once the two cases were named separately.
- **Two registers on the clip page** (Cliff, agreed — and it rehabilitates option C):
  - **Moment notes** (`Episode` variant, positioned, written in the flow while listening) render as **pins on the clip's waveform**, sourced by the derived span query. Option C was right as a *display* mechanism; the error was treating it as a substitute for targeting. Partial rehabilitation of H3's instinct.
  - **Clip notes** (`Clip` variant, no position, written about the artifact) are **the margin**: the layered dated stack.
  - Underlines in the text vs. writing in the margin. Rendering both as margin entries would have been wrong.
- **Narrowing is legible, not reconciled**: narrow the clip past a pin and the pin leaves it. Not data loss — the moment note still exists on the episode timeline, it just isn't inside this passage anymore, which is what narrowing a highlight means.
- **Pablo's other two calls (relayed via Cliff, 2026-07-26)**:
  - **The clip page does not play.** Playing is a separate explicit action that opens the main player. So there are no scrub handles; boundary editing is **sentence-extend** (include previous/next sentence). This fits the data better than the scrubber model did — `startMs`/`endMs` are already sentence-snapped at composer-commit (`Domain/Clip.swift:16-17`), so extend-by-sentence is the operation the data was always shaped for. **Does not touch the migration**; `Clip { clip_id }` doesn't care whether the page plays. Two-register display survives — the waveform is a static portrait rather than a scrubber.
  - **The margin's primary reader is you-in-six-months, not followers.** Private-first; sharing is a deliberate second act. Practical consequence for the notes work: **retrieval matters more than presentation** — search over margin text and resurfacing are worth more than anything on the share path.
- **The agent can read the source material and is blind to the margin.** Of 53 `AgentToolName` variants, only `CreateNote` and `CreateClip` touch notes/clips — both **writes**, no read, not even a wired-but-undefined one (Cliff's finding, verified). Sharper version: `QueryTranscripts` and `SearchEpisodes` **are advertised**, so the agent has semantic search over what the podcast said and zero access to what the user thought about it. An asymmetry, not just a gap — it can write into a layer it can never see again.
- **Clips are not Spotlight-indexed** (Cliff's finding, verified). `SpotlightIndexer` domains are `subscriptions` and `episodes` plus the notes/memories pass (`:10, :30-31, :117-138`); no clip path. A clip's frozen `transcriptText` is unfindable from system search even though notes are findable.
- **The semantic index is real but its corpus excludes the margin** — correction to a README-based claim, and the second time a doc promise outran the code (cf. `Clip.caption`). `pod0-recall-index` is substantial and implemented (schema, migration, cache, query, readiness, recovery tests; written from `runtime_evidence_commands.rs`), but its unit is `EvidenceSpanId` — **transcript spans, keyed per episode**. Zero note references in the crate; the lone "clip" hit in `query.rs` is a `clippy::too_many_arguments` attribute. So README:18's "local semantic indexing, hybrid retrieval, highlights, notes, and clips" describes the machinery accurately and the coverage aspirationally. Putting notes/clips in the recall corpus is real work, not wiring.
- ~~Possible cheap bridge via `spanID` on `NoteEvidence`/`ClipEvidence`.~~ **Investigated and downgraded.** The types do carry `spanID` (`Note.swift:88`, `Clip.swift:122`, `notes.rs:58`), but:
  - **Evidence has zero producers — it is an unwritten field, not a low-coverage one.** The only `NoteEvidence(`/`ClipEvidence(` constructions in Swift are `SharedNoteMapping:115` and `SharedClipMapping:101`, both **projection reads**; nothing in `AppTests`. Rust has no `EvidenceReference` construction outside domain/codec/tests (`facade_exports.rs:75,82` is a re-export, not a producer).
  - **No producer is available to add cheaply.** Cliff proposed `ClipBoundaryResolver` as an obvious one — but Swift's `Transcript.Segment` carries a locally-minted `id: UUID` with a `= UUID()` default (`Transcript.swift:57-63`), and there is **zero** `spanID`/`SpanId`/`span_id` anywhere in `App/Sources/Transcript/`. The resolver knows time ranges and local UUIDs; it never had a span to discard.
  - **`EvidenceSpanId` is content-hash-derived** (`pod0-domain/src/knowledge_identity.rs`, `from_bytes(first_16(hash.finish()))`), and there is **no time-range→span lookup anywhere in the workspace**. Populating evidence needs a span-resolution capability that does not exist, and it belongs in Rust (which owns evidence). Not a migration, but well above "almost free".
- **Span identity is a function of chunking policy, not of the transcript** (Cliff's finding, verified at `knowledge_identity.rs:80-97`). `evidence_span_id` hashes `transcript_version_id`, `content_digest`, **`policy_version`, `target_tokens`, `overlap_per_mille`, `snap_tolerance_per_mille`**, first/last `TranscriptSegmentId`, ordinals, ms range, and text. So resolution is not "which span covers time T" but "which span **under the current policy**", re-minting when policy changes — a capability with **versioning semantics**, not a lookup. It also needs Rust-side canonical `TranscriptSegmentId`s, which Swift should not be synthesizing (Rust owns identity). Cost estimate revised upward again.
- **Both resurfacing mechanisms are blocked, by different missing capabilities**: mechanism 1 (agent citation) on a read tool the agent surface does not have; mechanism 2 (contextual) on the span-resolution capability above. **Resurfacing is a genuine body of work, not a slice-2 add-on.**
- **The distinction that explains two wrong cost estimates** (Cliff's, agreed — worth carrying into any write-up): the kernel being ahead of its surfaces means things are **built**, not that they are **reachable**. Collapsing those produced "almost free" twice, wrongly.
- **PATTERN worth naming to Pablo — the kernel is consistently ahead of the surfaces that feed it.** Three independent instances found in one session: `Clip.caption` (threaded through create *and* update in the kernel, zero writers), `create_clip` (contract + executor + fingerprint, never advertised), `NoteEvidence`/`ClipEvidence` (domain type + storage codec + mapping + export, never populated). Also adjacent: `RecallAnswerService` has no production caller (per `@clay-claude`/`@meadow-codex` in `/pod0`), and the recall corpus excludes the margin. Not three coincidences.
- **Correction to Cliff's strongest-signal claim**: they argued the resurfacing split is corroborated by capture affordances ("only clips know their span at capture time"). **It does not hold** — a point maps to a covering span exactly as easily as a range, so both capture paths are equally span-blind. The UX derivation stands on its own; it just does not get a second independent derivation.
- **Retrieval infra that already exists** (so this is building on, not inventing): notes are Spotlight-indexed (`Services/SpotlightIndexer.swift:107`), and `AgentNotesView` has full-text search plus kind filters and date bucketing. **Missing: resurfacing** — nothing does it today. Likely the highest-compounding piece, and the one place a clip note and a moment note may want different treatment.
- **Non-bug, verified independently on both paths**: `ChapterRailItem.sortTime`'s `else { return 0 }` (`PlayerChaptersScrollView+Notes.swift:24`, and `:40` for the rendered position) looks like it would pile every clip note at 0:00, but it is unreachable — both note-by-episode accessors filter on `case .episode` first (`AppStateStore+Notes.swift:53-54`, `SharedLibraryClient+Notes.swift:29-33`), and the rail sources from `PlayerView.episodeNotes:299-302`. Clip notes never reach `sortTime`.
- **Real but benign product gap**: clip notes will be **invisible on the chapter rail**. Defensible under the split. Cliff's fix — give `ChapterRailItem` a `.clip` case so clips appear as rail structure alongside chapters, with their notes hanging off the clip rather than floating on the timeline — is additive, Swift-only, and independent of the migration.
- **Scope correction (Pablo)**: note→clip targeting requires **zero change to the `Clip` data structure** — no field, no back-reference. Caption/title was bundled into the same decision in error; it is a separate question about how a clip labels itself, now **off the critical path** and decidable independently or never.
- **Sequencing, converged between agents, pending Pablo**: slice 1 = clip page (bounded playback, boundary handles, title entry) — no schema change, no FFI risk. Slice 2 = `NoteTarget::Clip` migration; the margin lands on a page that already exists and has been used.
- **RESOLVED by @meadow-codex (owner of #56, authoritative)**: `PRODUCT_PROOF_AGENT_TOOLS` is a **milestone-scoped shipping allowlist** — a real current gate but not a permanent invariant. Commit `9ed3b87c` moved the prior Swift bounded product-proof schema surface into Rust unchanged; #56 excludes broad agent autonomy. `CreateClip` never had a provider schema there, and there is **no explicit safety or product decision withholding it**. Expanding the list is an intentional product-surface change, not cleanup. Confirms the "nobody got to it" read over "someone gated it"; @meadow-codex will not expose it under #56/#84.
- **Resurfacing asymmetry (Cliff, agreed)**: **clip notes resurface; moment notes do not.** A clip note is freestanding — the quote travels with it, so it survives out of context. A moment note is positional; out of context it is a fragment. And moment notes are *already* resurfaced by the chapter rail, which puts them back exactly when the episode is replayed — restoring context rather than stripping it. A second mechanism would make them worse. **The unit of resurfacing is the annotated clip.** Follows from Pablo's third call: Readwise resurfaces highlights because most people never annotate; the thesis here is that the annotation *is* the artifact, so what compounds is not "a quote you liked" but "what you thought".
- **Mechanism ranking (Cliff's, agreed)**: (1) **agent citation** — best, needs no new UI, currently *structurally impossible* per the read-tool asymmetry above; (2) **contextual** — good (new episode of an annotated show, or clipping semantically near an old clip), cost revised upward since the corpus excludes the margin; (3) **time-based daily review** — weakest; speech skims worse than book prose and it risks reading as homework.
- **Separate case, do not conflate — unannotated clips are INVITATION, not resurfacing.** Resolves the capture/annotation tension: capture happens in headphones while walking, annotation needs leisure. **Hard constraint: no count, no badge, no streak.** The instant it shows a growing number it becomes debt and gets abandoned. One at a time, skipping free and costless. *"An abandoned margin is worse than no margin."* Cliff flags this as the only part of the design where a wrong choice actively destroys value rather than merely failing to add it — agreed.
- **Still open**: caption→title naming (now off the critical path), agent tool shape (`create_note` optional target vs `annotate_clip`), sequencing of the H2 degrade-don't-drop fix, and whether `create_clip` should be exposed to the agent at all (it is currently dead-but-implemented).

## Follow-Up Artifacts

- (none yet)

## Follow-Up Artifacts

- **PR #171** — `NoteTarget::Clip { clip_id }`, schema 33, merged 2026-07-27. Two commits: `e16ce426` (degrade-don't-drop, independent bug fix) and `5db353ea` (the variant).
- **Issue #173** — the recovery screen tells users to reopen when reopening can never work. Found while shipping #171, not caused by it.
- Clip viewer and the writable margin: `@cliff-claude`.

## What The Estimates Got Wrong

Recorded because the pattern repeated and is the most portable thing here.

- **"One additive column."** Stated three times, to Cliff and to Pablo. It was a full table rebuild — `pod0_notes` carries `CHECK(target_code IN (0,1,2,255))` and SQLite cannot alter a CHECK in place. Estimated from the codec's shape without reading the DDL.
- **"Almost free" (Cliff, twice).** Once on evidence population, once on the semantic-index bridge. Both rested on machinery existing rather than being reachable.
- **The mirroring claim.** Cliff argued the resurfacing split was corroborated by capture affordances; it wasn't, because a point maps to a covering span as easily as a range does. Withdrawn.
- **"Deliberate curation" of the agent tool list.** Built from n=1; the denominator (~41 wired-but-never-defined vs 1 defined-but-withheld) killed it.

Every one was caught by checking a claim rather than accepting it, and in three of four cases the person who made the claim was the one who had read the most code. **Built ≠ reachable** is the generalization — see the `pod0-kernel-leads-surfaces` memory.
