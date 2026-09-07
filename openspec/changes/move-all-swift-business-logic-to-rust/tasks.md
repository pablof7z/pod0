## 1. Reconcile Scope and Freeze the Baseline

- [x] 1.1 Reconcile the applied subsystem-removal and repository-state changes, record overlapping files, and verify no later task restores deleted behavior.
- [x] 1.2 Expand the ownership inventory to cover every production Swift, Kotlin, and Rust file plus each mutable fact, durable store, command, projection, effect, observation, and recovery path; verify the inventory reports no missing or multiply owned entry.
- [ ] 1.3 Capture the pre-migration test, architecture-check, storage-version, startup/recovery, projection-pagination, playback-observation, and provider-payload baselines; verify the evidence is reproducible from documented commands.
- [ ] 1.4 Map every active Swift business-policy cohort and each of the eleven temporary exceptions to its target Rust owner or an explicit deletion task; verify the mapping has no unassigned item.
- [ ] 1.5 Define the per-wave authority-marker, backup, migration, validation, and legacy-deletion checklist; verify a fixture proves an interrupted cutover resumes without enabling two writers.

## 2. Build the Rust Transition and Effect Substrate

- [x] 2.1 Add stable command identity, expected-revision context, and typed applied/rejected/stale/duplicate/not-allowed/already-complete/no-op/cancelled/failed/outcome-unknown dispositions to the Rust application contract; verify replay and invalid-input unit tests pass.
- [ ] 2.2 Make state, semantic facts, idempotency receipts, internal commands, and external-effect intents commit atomically; verify fault injection at every commit boundary exposes either the old state or the complete new transition.
- [ ] 2.3 Implement the durable typed internal-command outbox with correlation, causation, idempotent target consumption, and wake-after-commit behavior; verify crash/replay tests do not lose or duplicate a target-domain mutation.
- [ ] 2.4 Implement immutable native-effect requests with effect, lease, attempt, fence, and cancellation identity; verify unleased, expired, duplicate, and stale-fence claims are rejected.
- [ ] 2.5 Implement bounded raw native observations and Rust-owned semantic outcome mapping; verify fixtures distinguish success, rejection, retryable evidence, cancellation, and outcome unknown without native booleans becoming product truth.
- [ ] 2.6 Implement correlated cancellation and exact-operation recovery for long-running capabilities; verify late callbacks and superseded attempts cannot mutate product state.
- [ ] 2.7 Add bounded batch query and projection primitives for hot native reads; verify baseline-sized and large-history benchmarks meet the documented limits.
- [ ] 2.8 Regenerate Swift and Kotlin bindings for the substrate and verify both compile and pass parity fixtures for ids, integer units, enums, bounded collections, and typed failures.

## 3. Make Boundary Enforcement Complete

- [ ] 3.1 Replace temporary-file-only checking with semantic scans of all production Swift and Kotlin sources for policy, direct durable writes, semantic fact construction, and direct effect dispatch; verify known violations outside the exception list fail the checker.
- [ ] 3.2 Add negative fixtures for unregistered inputs, arbitrary mutation closures, wildcard routing, native defaults/fallback/retry, fabricated activity, in-memory-only authorization, stale observations, and restored retired writers; verify each fixture fails for its intended rule.
- [ ] 3.3 Make the inventory and generated-binding parity checks mandatory in CI; verify a missing owner and a one-platform binding change each fail the required job.
- [ ] 3.4 Change the exception ratchet to reject every new native business-policy exception and require same-wave deletion of resolved exceptions; verify the final empty exception set passes.
- [ ] 3.5 Add a source-file length gate with a 300-line warning and 500-line failure for maintained source files; verify representative over-limit fixtures produce the expected result.

## 4. Migrate Settings, Categories, Credentials, and Usage

- [ ] 4.1 Define versioned Rust schemas and transitions for durable settings, defaults, validation state, revisions, and sync-conflict evidence; verify deterministic local and remote merge tests pass.
- [ ] 4.2 Import the legacy `Settings.swift` and AppState/iCloud-backed values once, commit the settings authority marker, and disable native writes; verify clean, populated, conflicting, and interrupted upgrade fixtures preserve one writer.
- [ ] 4.3 Move categories, category membership and overrides, category settings, and auto-download policy into a Rust owner; verify membership, override, default, and conflict scenario tests pass.
- [ ] 4.4 Replace category and settings UI mutations with typed Rust intents and bounded projections; verify iOS interaction tests change committed Rust state without touching a native product store.
- [ ] 4.5 Keep Keychain/OAuth material native behind opaque credential handles while moving connection metadata, authorization, validation state, and missing-credential interpretation to Rust; verify secrets never appear in Rust persistence, logs, facts, or projections.
- [ ] 4.6 Move provider key validation, TTS-preview eligibility, voice/model catalogs, selection, and BYOK policy from settings surfaces into Rust-authored requests and projections; verify native code supplies no provider/model/voice default.
- [ ] 4.7 Define the Rust usage/cost schema, causal identity, retention, and bounds, preserving the current supported legacy policy until separately changed; verify duplicate provider observations create at most one usage record.
- [ ] 4.8 Import `CostLedger` and `UsageRecord` data, cut all provider call sites to Rust accounting, and delete the native writer after validation; verify 90-day, 500-record, interruption, and backup/restore fixtures pass.

## 5. Move Transcript Meaning into Rust

- [ ] 5.1 Define bounded raw transcript transport envelopes for publisher files, remote providers, and platform speech observations; verify oversize, malformed-metadata, and unsupported-envelope fixtures are rejected before product mutation.
- [ ] 5.2 Port transcript format qualification, parsing, segment/word normalization, ordering, and provenance from native parsers into Rust; verify existing format fixtures produce stable canonical output.
- [ ] 5.3 Move speaker identity and stable segment/word identity into Rust; verify repeated imports and provider reorderings retain deterministic identities.
- [ ] 5.4 Move canonical transcript selection, artifact validation, and semantic failure/retry interpretation into the Rust workflow; verify accepted-then-failed, cancelled, unsupported, and ambiguous provider scenarios preserve correct phase truth.
- [ ] 5.5 Reduce transcript provider clients and Apple Speech integration to exact leased byte/audio/observation capabilities with raw transport evidence; verify native providers do not normalize, retry, fall back, or classify product outcomes.
- [ ] 5.6 Remove provider-phase fabrication and failure mapping from `CoreTranscriptHost`, host failures, and `TranscriptObservationMapper`; verify phase is derived exclusively from committed Rust evidence.
- [ ] 5.7 Delete superseded native transcript parsers, normalization helpers, and their exceptions after migration; verify repository search and the zero-exception checker find no alternate implementation.

## 6. Move Agent, Voice, and Provider Policy into Rust

- [ ] 6.1 Define one Rust turn model for text and voice with prompts, context, model/provider selection, proposals, approvals, tools, cancellation, recovery, generation, usage, and publication; verify equivalent text/voice intent fixtures use the same transition rules.
- [ ] 6.2 Move system-prompt construction, capability selection, retrieval/context policy, and model selection from `AgentPrompt`, `SharedAgentConversationSession`, and shared clients into Rust projections and effect requests; verify identical state yields identical requests on Swift and Kotlin.
- [ ] 6.3 Replace `AgentApprovalCoordinator` automatic approval with correlated proposal, approve, deny, and dismiss observations; verify no approval-required effect can be leased before the matching committed approval.
- [ ] 6.4 Reconcile `agent-tool-permissions.json` so product-mutating tools dispatch durable Rust internal commands and only literal platform capabilities dispatch native leases; verify every registered tool has exactly one target owner and authorization path.
- [ ] 6.5 Replace direct playback, rate, library, settings, note, clip, and workflow mutations in `CoreAgentCapabilityExecutor` with target-domain internal commands; verify denial, duplicate, stale, crash, and replay tests pass.
- [ ] 6.6 Move conversation phase, interruption, barge-in, fallback, and default-voice policy from `AudioConversationManager`, `VoiceTypes`, and `ElevenLabsTTS` into Rust; verify late speech/model callbacks are fenced after cancellation.
- [ ] 6.7 Replace direct provider orchestration in `VoiceNoteRecordingSheet` and related voice-note paths with Rust commands and leased speech effects; verify recording cancellation and provider failure leave recoverable Rust state.
- [ ] 6.8 Reduce LLM, STT, TTS, generated-media, and publication clients to exact request execution with raw bounded observations; verify a provider failure triggers no native retry or provider switch.
- [ ] 6.9 Remove `UtilityLLMClient`, `UtilityLLMFailureMapping`, and other superseded prompt/model/failure policy after callers cut over; verify no production references or architecture exceptions remain.

## 7. Migrate Workflow, Scheduling, and Background Work

- [ ] 7.1 Move workflow configuration, import identity, validation, allowed actions, and reconciliation out of `WorkflowRuntime` and native configuration types into Rust; verify create/import/update/reconcile fixtures are deterministic and idempotent.
- [ ] 7.2 Move scheduled-task identity, interval validation, model validation/selection, and next-occurrence calculation into Rust; verify timezone, overdue, duplicate, invalid interval, and restart fixtures pass.
- [ ] 7.3 Treat BGTask and other operating-system wakes as native opportunity observations, with Rust selecting due work and issuing exact leases; verify duplicate and late wakes do not duplicate scheduled runs.
- [ ] 7.4 Replace `BackgroundWorkScheduler` policy fanout with a bounded Rust work projection and literal native registration/execution; verify native code cannot add, reorder, or silently drop product work.
- [ ] 7.5 Import any supported legacy workflow and schedule state once, commit the authority marker, and remove native writers; verify interrupted migration and restore fixtures pass.
- [ ] 7.6 Delete legacy `JobStore`, `ArtifactRepository`, `DesiredStatePlanner`, `LegacyWorkflow`, and migration-only adapters when their supported imports are complete; verify no production references or compatibility aliases remain.

## 8. Migrate Library, Feed, Search, and Import Policy

- [ ] 8.1 Move episode metadata derivation, library/home eligibility and ordering, and the continue-listening window from AppState and display helpers into Rust projections; verify large-history and boundary-date fixtures return stable bounded results.
- [ ] 8.2 Move subscription/feed admission, identity, deduplication, refresh outcome, and retry policy into the Rust owner while leaving URLSession transport native; verify duplicate, stale, malformed, auth, rate-limit, and recovery scenarios pass.
- [ ] 8.3 Replace native host failure mapping in `CoreFeedHost` and `CoreLibraryNetworkHost` with raw HTTP/platform observations; verify Rust alone derives semantic feed/library outcomes.
- [ ] 8.4 Move product search qualification, ranking, tie-breaking, and result bounds from `PodcastSearchModels` and related clients into Rust; verify fixed-corpus ranking is identical through Swift and Kotlin bindings.
- [ ] 8.5 Move OPML parsing decisions, import identity, conflict handling, and export selection into Rust-authored import/export plans; verify duplicate feeds, malformed entries, round-trip, and interrupted import fixtures pass.
- [ ] 8.6 Replace `SharedEpisodeImportCoordinator` mutation sequences with one Rust import transition and durable downstream commands; verify a crash cannot partially import or duplicate requested downloads.
- [ ] 8.7 Delete the uncalled native `FeedClient`/RSS parser chain and other superseded library/search policy after reference proof; verify production builds and repository search confirm removal.

## 9. Migrate Playback and Download Policy

- [ ] 9.1 Define Rust playback state and atomic commands for selection, queue replacement, immediate play, pause, seek, rate, completion, and supersession; verify transition and stale-revision tests pass.
- [ ] 9.2 Move queue construction, headphone-action fallback, auto-delete/auto-download consequences, and playback-triggered workflow policy into Rust; verify cross-domain work is emitted only as durable internal commands.
- [ ] 9.3 Reduce `Controls`, `Queue`, `HeadphoneGestures`, `SharedCore`, AppState playback mutations, and episode metadata helpers to intents, projections, and literal media execution; verify iOS playback flows render committed Rust state.
- [ ] 9.4 Move recall handoff and player-share selection into Rust-authored plans; verify handoff/share fixtures cannot change item selection or provenance in native code.
- [ ] 9.5 Replace `CoreDownloadHost` and delegate failure/retry mapping with leased exact downloads, correlated cancellation, raw HTTP/platform evidence, and bounded resume handles; verify crash, cancellation, stale callback, and ambiguous-completion tests pass.
- [ ] 9.6 Delete superseded Swift playback/download stores, defaults, retry paths, and policy helpers after cutover; verify no native writer or exception remains.

## 10. Migrate Export, Share, Clip, Index, and Publication Plans

- [ ] 10.1 Define versioned Rust full-data export plans covering every authoritative domain, redaction, provenance, ordering, naming, and bounds; verify golden fixtures cover populated, empty, large, and partially unavailable data.
- [ ] 10.2 Replace selection and policy in `DataExport` and `AgentChatTranscriptExport` with exact plan rendering and native file writes; verify exported content matches the Rust plan byte-for-byte apart from declared platform encoding.
- [ ] 10.3 Move clip boundaries, selected media, metadata, and generation/adoption policy from `ClipBoundaryResolver`, `ClipExporter`, and `ClipAudioComposer` into Rust; verify staged audio is invisible until a matching Rust adoption transition commits.
- [ ] 10.4 Move Spotlight item selection, identifiers, metadata, and retention into a Rust index plan while keeping Spotlight API execution native; verify native indexing cannot invent or omit planned items.
- [ ] 10.5 Delete the unused legacy Spotlight reindex path after reference proof; verify production builds and source search confirm removal.
- [ ] 10.7 Reduce surviving signing/publication/network code to exact leased transport and raw receipts; verify mixed sent/acknowledged/rejected/retry/terminal evidence is interpreted only by Rust.

## 11. Remove Native Semantic Host Behavior

- [ ] 11.1 Audit `CoreAgentHost`, `CoreRecallHost`, `CoreFeedHost`, `CoreLibraryNetworkHost`, `CoreDownloadHost`, `CoreTranscriptHost`, and chapter transports against the exact request/raw observation contract; verify each method is classified as compliant, migrated, or deleted.
- [ ] 11.2 Remove native retry, backoff, fallback, provider/model selection, default insertion, and semantic failure construction from all surviving hosts; verify injected raw failures produce no unrequested second attempt.
- [ ] 11.3 Ensure staged files, buffers, resume data, widget snapshots, and platform handles are non-authoritative and require Rust adoption; verify orphaned and mismatched artifacts never enter projections.
- [ ] 11.4 Keep layout, localization, accessibility, navigation, haptics, media frameworks, speech frameworks, Keychain prompts, OAuth UI, notifications, widgets, share sheets, and literal file operations native; verify architecture rules accept these adapters only while they remain policy-free.
- [ ] 11.5 Split touched source files at ownership seams where practical and below the 500-line hard limit; verify the length gate passes and no serif font APIs are introduced.

## 12. Delete Dormant and Superseded Policy

- [ ] 12.1 Delete the uncalled `AgentAskCoordinator`/presenter owner-question infrastructure after confirming ordinary conversation and current question flows do not depend on it; verify production builds and targeted UI tests pass.
- [ ] 12.2 Delete the unused review-prompt path after confirming no production references; verify source search and affected tests pass without a replacement.
- [ ] 12.3 Remove all migration-complete readers, compatibility branches, aliases, forwarding APIs, obsolete caches, and policy-specific tests in the same wave as their Rust replacement; verify the legacy-symbol denylist is empty.
- [ ] 12.4 Remove every resolved row from the Swift exception manifest and delete the manifest when empty; verify the architecture gate rejects reintroducing each retired pattern.

## 13. Prove the Zero-Exception End State

- [ ] 13.1 Run per-domain manual, automatic, playback-triggered, agent-triggered, ingestion, scheduled, retry, recovery, cancellation, denial, rejection, and failure scenarios; verify every conformance-matrix row has reproducible evidence.
- [ ] 13.2 Run fault injection across admission, transition commit, effect claim, execution, observation commit, internal-command delivery, projection publication, migration, backup, restore, and erasure; verify one committed outcome or explicit outcome unknown at every seam.
- [ ] 13.3 Run concurrency tests for duplicate commands, stale revisions, stale fences, lease expiry, cancellation, supersession, and observation races; verify no duplicated unsafe effect or conflicting durable writer.
- [ ] 13.4 Regenerate and validate Swift and Kotlin bindings, then run Rust, binding, iOS, and Android-compatible test suites; verify all supported clients exercise the same contract variants.
- [ ] 13.5 Run the complete ownership inventory, semantic architecture checks, negative fixtures, zero-exception gate, legacy-symbol denylist, and file-length gate; verify every required check passes from a clean checkout.
- [ ] 13.6 Re-run startup/recovery, write-amplification, pagination, playback-observation, provider-response, native-buffer, and large-history benchmarks; verify all documented bounds meet or improve the frozen baseline.
- [ ] 13.7 Perform supported upgrade, backup, restore, and forward-only rollback rehearsals with realistic legacy data; verify user state is preserved and no path re-enables Swift authority.
- [ ] 13.8 Assemble the final #204 and #213-#219 proof package linking implementation, migration, deletion, tests, and benchmark evidence; verify every claimed closure is backed by a passing command or observable artifact.
