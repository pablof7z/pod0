# BDD Catalog 06 - Expanded Product Validation

These scenarios fill the target catalog v2 gaps from the Pod0 BDD expansion
plan. They focus on user-visible product completeness, replayability, evidence
quality, and NMP/RMP boundaries. Every row remains incomplete until current
screenshots, UI trees, metrics, cassettes, issues, and revalidation evidence are
attached to its generated scenario page.

## Foundation And First Run Expansion

| ID | Scenario | Evidence |
|---|---|---|
| FND-017 | Given notifications are requested during first run, when the user denies permission, then onboarding continues and Settings shows the denied notification state truthfully. | SS: permission prompt, denied onboarding state, Settings notification row, UI tree; Perf: denial-to-state under 500 ms; Deps: simulator notification denied fixture; Boundary: D6,D7. |
| FND-018 | Given an OPML file is offered during onboarding, when import contains valid and invalid feeds, then valid subscriptions appear and invalid rows remain recoverable. | SS: OPML picker, import summary, Library result, UI tree; Perf: import summary under 2 sec; Deps: mixed OPML and RSS cassettes; Boundary: D4,D6,D7. |
| FND-019 | Given identity setup is deferred, when onboarding finishes, then the listener can browse and play without hidden signing prompts. | SS: deferred identity step, Home, playback start, UI tree; Perf: final transition under 500 ms; Deps: erased sim and seeded starter feed; Boundary: D4,D6,D10. |
| FND-020 | Given starter feed discovery returns a failure, when onboarding suggestions load, then the page shows a recoverable empty state instead of blocking first run. | SS: suggestion failure, retry, skip action, UI tree; Perf: failure state under 1 sec; Deps: starter feed failure cassette; Boundary: D1,D6,D7. |
| FND-021 | Given a podcast deep link opens before onboarding is complete, when onboarding finishes, then the app routes once to the linked episode without duplicate navigation. | SS: deep-link launch, final onboarding page, routed episode, UI tree; Perf: route under 2 sec after final snapshot; Deps: cold deep-link fixture; Boundary: D4,D5,D7. |
| FND-022 | Given the device locale is non-English and 24-hour time is enabled, when onboarding and Home render, then dates, durations, and copy stay readable without layout overlap. | SS: localized onboarding and Home, UI tree; Perf: layout settle under 250 ms; Deps: locale simulator setting; Boundary: native render,D5. |
| FND-023 | Given an iCloud or app-group restore has existing subscriptions but no onboarding marker, when Pod0 launches, then restore is recognized and onboarding does not destroy data. | SS: restored Home, restore banner, Library, UI tree; Perf: restore recognition under 1 sec; Deps: restored app-group fixture; Boundary: D4,D5,D10. |
| FND-024 | Given VoiceOver is enabled on first launch, when the user navigates onboarding, then every page title, primary action, progress indicator, and skip action is reachable and labeled. | SS: onboarding accessibility screen set, UI tree with labels and traits; Perf: none; Deps: VoiceOver UI-tree capture; Boundary: native render,D1. |

## Discovery And Search Expansion

| ID | Scenario | Evidence |
|---|---|---|
| DISC-017 | Given provider search is enabled and directory search times out, when the user escalates to provider search, then replayed provider results render with source citations and no live-call fallback. | SS: timeout, escalation card, provider results, UI tree; Perf: escalation feedback under 500 ms and replay latency; Deps: directory timeout and provider search cassette; Boundary: D6,D7,D10. |
| DISC-018 | Given one search query has results, when the query is cleared and a new query starts, then stale results disappear before new scoped results render. | SS: old results, cleared state, new results, UI tree; Perf: stale clear under 150 ms; Deps: scoped search replay; Boundary: D4,D5,D8. |
| DISC-019 | Given a Discover result deep link opens from another app, when Pod0 is cold, then the correct show detail opens without flashing previous search state. | SS: cold open, show detail, search view revisit, UI tree; Perf: route under 2 sec after first snapshot; Deps: deep-link and feed cassette; Boundary: D4,D5,D7. |
| DISC-020 | Given a no-result search returns from local, directory, and provider layers, when the final state renders, then copy explains all attempted scopes and offers a recoverable next action. | SS: no-result layers, recovery CTA, UI tree; Perf: final state within budget; Deps: no-result directory and provider cassettes; Boundary: D6,D7. |

## Library And Episode Management Expansion

| ID | Scenario | Evidence |
|---|---|---|
| LIB-017 | Given Library sort is changed to oldest-first, when the app relaunches, then the same sort renders before any manual refresh. | SS: sort menu, ordered list, relaunch list, UI tree; Perf: relaunch render under 1 sec; Deps: multi-show seed; Boundary: D4,D5. |
| LIB-018 | Given a filter and category are active together, when one is cleared, then the remaining scope stays visible and mechanically derives from the kernel projection. | SS: combined filters, clear action, remaining scope, UI tree; Perf: filter update under 300 ms; Deps: categorized library seed; Boundary: D4,D5. |
| LIB-019 | Given a user edits a category name, when the edit is saved, then every affected show row updates and Android/TUI decode logs match the new category. | SS: edit form, Library rows, parity decode log, UI tree; Perf: projection update under 500 ms; Deps: category fixture; Boundary: D4,D5. |
| LIB-020 | Given feed refresh adds and removes episodes, when refresh completes, then new rows appear, removed rows are marked unavailable, and playback progress is not clobbered. | SS: before refresh, after refresh, unavailable row, UI tree; Perf: refresh settle timing; Deps: RSS delta cassette; Boundary: D4,D6,D7. |
| LIB-021 | Given OPML export includes private feed URLs, when export preview is linted, then private tokens are redacted or excluded before sharing. | SS: export preview, redaction lint output, UI tree; Perf: export time; Deps: private feed seed; Boundary: D10. |
| LIB-022 | Given projection decode fails for a Library sidecar, when the next good frame arrives, then resync clears and the visible Library state becomes current. | SS: resync indicator, recovery frame, Library result, UI tree; Perf: recovery under 1 sec after frame; Deps: malformed then valid projection replay; Boundary: D5,D6,D8. |
| LIB-023 | Given artwork fetch returns a large image and a broken image, when Library scrolls, then placeholders and decoded artwork remain smooth without native business cache divergence. | SS: artwork placeholder, loaded artwork, broken artwork, UI tree; Perf: scroll FPS and memory; Deps: artwork HTTP cassettes; Boundary: D4,D5,D8. |
| LIB-024 | Given a show is archived instead of unsubscribed, when Library filters are changed, then archive visibility follows the Rust-owned filter state across relaunch. | SS: archive action, hidden filter, archived filter, relaunch, UI tree; Perf: filter update under 300 ms; Deps: archive fixture; Boundary: D4,D5. |

## Playback Expansion

| ID | Scenario | Evidence |
|---|---|---|
| PLAY-017 | Given audio route changes from speaker to headphones, when the route event arrives, then the player shows the raw route state without changing playback policy in Swift. | SS: route state before/after, UI tree; Perf: route update under 500 ms; Deps: audio route capability replay; Boundary: D7. |
| PLAY-018 | Given an interruption begins during playback, when the OS reports interruption start, then playback state pauses and resume affordance reflects Rust policy. | SS: interruption banner, paused player, UI tree; Perf: interruption response under 500 ms; Deps: audio interruption replay; Boundary: D6,D7. |
| PLAY-019 | Given an interruption ends with should-resume, when the OS reports completion, then playback resumes only through the kernel decision path. | SS: interruption end, resumed player, UI tree; Perf: resume under 1 sec; Deps: interruption end replay; Boundary: D4,D7. |
| PLAY-020 | Given artwork fails to load in Now Playing, when playback starts, then player chrome uses a stable fallback and does not block transport controls. | SS: full player fallback, mini-player fallback, UI tree; Perf: player render under 500 ms; Deps: artwork failure cassette; Boundary: D1,D6. |
| PLAY-021 | Given speed is changed while an episode is playing, when the next episode auto-plays, then the selected speed persists if policy says global speed. | SS: speed sheet, next episode, speed label, UI tree; Perf: transition gap; Deps: two-episode queue fixture; Boundary: D4,D9. |
| PLAY-022 | Given per-show speed override exists, when switching shows, then the player uses the correct show-specific speed without stale global value. | SS: first show speed, second show speed, UI tree; Perf: speed projection under 500 ms; Deps: per-show settings fixture; Boundary: D4,D5. |
| PLAY-023 | Given a chapter has artwork and a URL, when the active chapter changes, then chapter metadata updates without forcing a full player rebuild. | SS: chapter list, active chapter metadata, UI tree; Perf: chapter update under 250 ms; Deps: chapter metadata fixture; Boundary: D4,D5,D8. |
| PLAY-024 | Given chapter metadata is malformed, when playback crosses that boundary, then the player degrades to episode metadata and logs the malformed chapter. | SS: fallback metadata, log excerpt, UI tree; Perf: no playback gap; Deps: malformed chapter fixture; Boundary: D6,D7. |
| PLAY-025 | Given a lock-screen remote command seeks forward, when Pod0 receives it, then elapsed time and transcript active line update coherently. | SS: app after remote seek, transcript line, UI tree; Perf: command latency; Deps: remote command harness; Boundary: D4,D7. |
| PLAY-026 | Given AirPlay output is selected, when playback starts, then route state is visible and local UI controls remain synchronized. | SS: route picker state, full player, UI tree; Perf: route start latency; Deps: route capability replay; Boundary: D7. |
| PLAY-027 | Given CarPlay is connected during playback, when the queue changes, then CarPlay and app player show the same current and next episode. | SS: CarPlay player, app queue, UI tree/log; Perf: queue sync under 1 sec; Deps: CarPlay harness and queue seed; Boundary: D4,D5,D7. |
| PLAY-028 | Given episode duration is unknown at start, when duration becomes available, then scrubber bounds update without jumping elapsed position. | SS: unknown duration, known duration, UI tree; Perf: duration update under 500 ms; Deps: delayed metadata replay; Boundary: D4,D6. |
| PLAY-029 | Given playback reaches end of episode with a transcript follow-along open, when next episode starts, then transcript state clears or switches without stale active line. | SS: end state, next episode, transcript tab, UI tree; Perf: transition gap; Deps: short queue and transcript seed; Boundary: D4,D5,D8. |
| PLAY-030 | Given a stream URL expires mid-playback, when playback reports failure, then Pod0 shows recoverable error and does not retry in a tight loop. | SS: playback error, retry control, UI tree; Perf: no repeated wake loop; Deps: expiring stream cassette; Boundary: D6,D7,D8. |
| PLAY-031 | Given a listener changes skip interval settings, when remote skip command fires, then the remote command uses the updated Rust-owned interval. | SS: settings, remote command result, elapsed label, UI tree; Perf: action under 250 ms; Deps: remote command harness; Boundary: D4,D7. |
| PLAY-032 | Given mini-player is visible across tab changes, when the user navigates Home, Library, Agent, and Settings, then mini-player state and controls remain coherent. | SS: mini-player on each tab, UI tree; Perf: tab switch under 250 ms; Deps: seeded player; Boundary: D4,D5,D8. |

## Queue And Downloads Expansion

| ID | Scenario | Evidence |
|---|---|---|
| QD-017 | Given multiple downloads have different priorities, when network becomes available, then download start order follows Rust policy and UI explains queue order. | SS: priority queue, active download, UI tree; Perf: start order timing; Deps: multi-download replay; Boundary: D4,D7. |
| QD-018 | Given storage eviction is needed, when auto-download policy runs, then eligible downloads are evicted with visible reason and protected episodes remain. | SS: storage warning, evicted row, protected row, UI tree; Perf: eviction timing; Deps: storage pressure fixture; Boundary: D4,D7,D10. |
| QD-019 | Given a downloaded file is missing on disk, when the downloads screen opens, then repair state appears before playback is attempted. | SS: missing file badge, repair action, UI tree; Perf: file scan budget; Deps: missing file fixture; Boundary: D6,D7. |
| QD-020 | Given repair is tapped for a missing file, when download restarts, then prior progress is cleared and a new capability request is visible. | SS: repair action, new progress, UI tree; Perf: restart under 500 ms; Deps: missing file and download replay; Boundary: D4,D7. |
| QD-021 | Given a private feed download URL has bearer query params, when diagnostics and screenshots are produced, then URL secrets are redacted. | SS: diagnostics redaction, download row, UI tree; Perf: none; Deps: private feed cassette; Boundary: D10. |
| QD-022 | Given a widget requests download summary, when many downloads are active, then only bounded summary counts cross to the widget. | SS: widget summary, download list, UI tree/log; Perf: widget update cadence; Deps: large download seed; Boundary: D5,D8. |
| QD-023 | Given Live Activity is active for downloads, when progress changes rapidly, then updates are coalesced and do not exceed the cadence budget. | SS: Live Activity, metrics log, UI tree; Perf: update cadence <= 1 Hz; Deps: progress replay; Boundary: D5,D8. |
| QD-024 | Given the user cancels all downloads, when background capability callbacks arrive late, then canceled rows do not resurrect. | SS: cancel all, late callback log, final list, UI tree; Perf: no stale mutation; Deps: delayed download callback replay; Boundary: D4,D7,D8. |

## Transcripts Expansion

| ID | Scenario | Evidence |
|---|---|---|
| TR-017 | Given a transcript has diarized speakers, when transcript view opens, then speaker names, timing, and active-line styling remain readable. | SS: diarized transcript, active line, UI tree; Perf: render under 1 sec; Deps: diarized transcript fixture; Boundary: D4,D5. |
| TR-018 | Given diarization labels are unknown, when transcript renders, then stable speaker placeholders are used and searchable. | SS: unknown speakers, search result, UI tree; Perf: search under 500 ms; Deps: unknown diarization fixture; Boundary: D6,D5. |
| TR-019 | Given STT returns partial segments, when generation progress renders, then partial text is labeled provisional and not indexed as final. | SS: provisional segments, progress, UI tree; Perf: partial render latency; Deps: partial STT cassette; Boundary: D4,D6,D7. |
| TR-020 | Given STT returns low-confidence words, when transcript finalizes, then low-confidence ranges remain visible and are excluded from high-confidence citations. | SS: confidence ranges, citation result, UI tree; Perf: none; Deps: low-confidence STT cassette; Boundary: D4,D6. |
| TR-021 | Given a long transcript is virtualized, when the user jumps from top to middle to end, then active segment and scroll position stay correct. | SS: top, middle, end, UI tree; Perf: scroll FPS and memory; Deps: five-hour transcript fixture; Boundary: D5,D8. |
| TR-022 | Given transcript index exists, when the app relaunches offline, then transcript search returns local snippets without provider calls. | SS: offline search, snippet result, UI tree; Perf: local search under 500 ms; Deps: indexed transcript fixture; Boundary: D4,D5,D7. |
| TR-023 | Given transcript search result is tapped while audio is playing, when route completes, then playback seeks and transcript follow-along resumes at that span. | SS: search result, player, active line, UI tree; Perf: route and seek under 500 ms; Deps: transcript and player seed; Boundary: D4,D5,D7. |
| TR-024 | Given publisher transcript fetch times out, when the user requests generated STT fallback, then fallback path is explicit and replayable. | SS: timeout state, fallback prompt, UI tree; Perf: timeout budget; Deps: transcript timeout and STT cassette; Boundary: D6,D7. |
| TR-025 | Given Apple STT permission is restricted by Screen Time or policy, when generation is requested, then the app shows restricted state and provider alternatives. | SS: restricted permission, alternatives, UI tree; Perf: none; Deps: restricted permission fixture; Boundary: D6,D7. |
| TR-026 | Given AssemblyAI polling exceeds max attempts, when replay clock advances, then generation fails with terminal state and no polling loop remains. | SS: poll progress, terminal timeout, UI tree; Perf: poll count and timeout; Deps: AssemblyAI timeout cassette and replay clock; Boundary: D6,D8,D9. |
| TR-027 | Given transcript export is requested, when export completes, then text, timestamps, and speaker labels are included without private provider payloads. | SS: export preview, redaction log, UI tree; Perf: export time; Deps: transcript export fixture; Boundary: D10. |
| TR-028 | Given malformed transcript JSON has overlapping segments, when ingest runs, then overlap is reported and safe segments still render. | SS: overlap warning, rendered safe segments, UI tree; Perf: parse under 1 sec; Deps: malformed transcript fixture; Boundary: D6,D7. |

## Clips, Highlights, And Sharing Expansion

| ID | Scenario | Evidence |
|---|---|---|
| CLIP-017 | Given sentence-boundary editing is enabled, when a clip handle lands mid-word, then the editor previews the snap target before saving. | SS: handle drag, snap preview, UI tree; Perf: preview under 150 ms; Deps: transcript seed; Boundary: D4,D8. |
| CLIP-018 | Given a clip references an episode that was later removed, when repair is offered, then the user can relink or keep an orphaned local clip. | SS: orphan repair, relink result, UI tree; Perf: repair route under 1 sec; Deps: orphan clip fixture; Boundary: D4,D6. |
| CLIP-019 | Given relay rejects a published clip for policy reasons, when user retries after editing, then local clip state preserves edit history and publish state updates. | SS: reject, edit, retry, UI tree; Perf: relay ACK timing; Deps: relay reject then accept cassette; Boundary: D6,D7. |
| CLIP-020 | Given image share card includes long show and episode names, when generated, then text wraps without overlap and retains source timestamp. | SS: image preview, UI tree; Perf: render time; Deps: long metadata clip seed; Boundary: native render,D5. |
| CLIP-021 | Given audio share export is canceled, when the share sheet closes, then no orphan export file remains and clip state is unchanged. | SS: cancel share, file cleanup log, UI tree; Perf: cleanup timing; Deps: export cancel fixture; Boundary: D4,D7. |
| CLIP-022 | Given unsafe quoted text contains script-like markup, when published and shared, then rendered text is escaped in app and event metadata. | SS: app preview, event metadata log, UI tree; Perf: none; Deps: unsafe text seed; Boundary: D10,native render. |
| CLIP-023 | Given a clip deep link targets a deleted local clip, when opened cold, then the app shows deleted/unavailable state and nearby recovery actions. | SS: cold deep link unavailable, recovery action, UI tree; Perf: route under 2 sec; Deps: deleted clip deep-link fixture; Boundary: D6,D5. |
| CLIP-024 | Given clip metadata is re-published after edit, when relay accepts the new event, then old and new event provenance is visible in diagnostics. | SS: edit publish state, diagnostics provenance, UI tree; Perf: ACK timing; Deps: relay accept cassette; Boundary: D3,D4,D7. |

## Voice Expansion

| ID | Scenario | Evidence |
|---|---|---|
| VOICE-019 | Given audio output route changes during TTS playback, when route event arrives, then speaking state remains coherent and route label updates. | SS: speaking state, route label, UI tree; Perf: route update under 500 ms; Deps: route change and TTS replay; Boundary: D7. |
| VOICE-020 | Given noisy input produces unstable partials, when STT finalizes, then only final accepted text is committed to chat history. | SS: partial noise, final text, chat history, UI tree; Perf: finalization latency; Deps: noisy input STT replay; Boundary: D4,D7,D10. |
| VOICE-021 | Given voice mode backgrounds during listening, when the app foregrounds, then listening is stopped or resumed according to explicit Rust state. | SS: background return state, VoiceView, UI tree; Perf: foreground settle under 500 ms; Deps: lifecycle replay; Boundary: D4,D7. |
| VOICE-022 | Given a Siri/App Intent starts a podcast command while locked, when Pod0 receives the intent, then the action is limited to safe playback/search state. | SS: intent log, safe result, UI tree; Perf: intent latency; Deps: App Intent fixture; Boundary: D7,D10. |
| VOICE-023 | Given offline local-only voice mode is enabled, when the user asks a local transcript question, then Pod0 answers from local index and labels online tools unavailable. | SS: offline answer, unavailable tools, UI tree; Perf: local answer under 1 sec; Deps: local STT and transcript fixture; Boundary: D6,D7. |
| VOICE-024 | Given TTS is canceled while audio playback continues, when the cancel action completes, then podcast playback route and elapsed state are unchanged. | SS: TTS cancel, player unchanged, UI tree; Perf: cancel under 250 ms; Deps: TTS cancel replay and seeded player; Boundary: D4,D7,D8. |

## Offline And Replay Honesty Expansion

| ID | Scenario | Evidence |
|---|---|---|
| OFFLINE-009 | Given replay mode is enabled and a cassette request fingerprint mismatches, when validation runs, then the provider call fails closed with a visible cassette-miss state. | SS: cassette miss page, validation log, UI tree; Perf: failure under 250 ms; Deps: mismatched cassette fixture; Boundary: D7,D10. |
| OFFLINE-010 | Given a cassette is older than the freshness budget, when provider replay validation runs, then the scenario page marks freshness blocked and no pass verdict is allowed. | SS: stale cassette report, provider matrix, UI tree; Perf: none; Deps: stale cassette fixture; Boundary: D7,D9. |
| OFFLINE-011 | Given offline transcript search runs, when a matching local transcript exists, then snippets render without any STT or LLM provider dependency. | SS: offline transcript search, snippet, UI tree; Perf: local search under 500 ms; Deps: offline transcript index fixture; Boundary: D5,D7. |
| OFFLINE-012 | Given offline agent mode has no provider cassette for a requested answer, when the user asks, then the agent explains replay coverage is missing and does not call live network. | SS: agent replay-missing state, UI tree; Perf: failure under 500 ms; Deps: missing LLM cassette fixture; Boundary: D6,D7,D10. |
| OFFLINE-013 | Given offline web search is requested, when no cached results exist, then Pod0 shows unavailable search state with local alternatives. | SS: unavailable online search, local alternatives, UI tree; Perf: state under 500 ms; Deps: offline network mock; Boundary: D6,D7. |
| OFFLINE-014 | Given a relay reconnect succeeds after pending publishes, when ACKs arrive, then pending state clears only for acknowledged events. | SS: pending list, partial ACK, final ACK, UI tree; Perf: reconnect-to-ACK timing; Deps: partial reconnect relay replay; Boundary: D3,D6,D8. |
| OFFLINE-015 | Given an uncached profile is requested offline from social, when the profile cannot resolve, then friend actions stay disabled with honest copy. | SS: uncached profile, disabled actions, UI tree; Perf: no indefinite spinner; Deps: offline profile fixture; Boundary: D6,D10. |
| OFFLINE-016 | Given an uncached show is opened offline from a deep link, when no metadata is available, then the app preserves navigation context and offers retry later. | SS: unavailable show, retry later, UI tree; Perf: route under 1 sec; Deps: offline deep-link fixture; Boundary: D1,D6. |
| OFFLINE-017 | Given offline playback starts from a downloaded episode, when artwork and transcript are missing, then audio still plays and missing metadata is labeled. | SS: player, missing artwork, missing transcript label, UI tree; Perf: start under 1 sec; Deps: downloaded episode fixture; Boundary: D1,D6,D7. |
| OFFLINE-018 | Given replay mode is enabled in CI, when any provider client attempts live network, then the verifier fails the run and links the scenario page. | SS: CI failure log, scenario link, UI tree if applicable; Perf: none; Deps: no-live-call CI fixture; Boundary: D7,D10. |
| OFFLINE-019 | Given reconnect happens while app is backgrounded, when app foregrounds, then outbox and diagnostics show the final relay state without stale pending rows. | SS: foreground outbox, diagnostics, UI tree; Perf: foreground settle under 500 ms; Deps: background reconnect replay; Boundary: D4,D6,D8. |
| OFFLINE-020 | Given network flaps repeatedly, when diagnostics is open, then backoff and connected states update without exceeding cadence budgets. | SS: diagnostics timeline, UI tree; Perf: update cadence <= 60 Hz; Deps: network flap replay; Boundary: D7,D8. |
| OFFLINE-021 | Given provider replay succeeds instantly, when the user views result timing, then the UI labels replay mode and does not imply live provider performance. | SS: replay badge, timing metrics, UI tree; Perf: replay latency budget; Deps: provider cassette suite; Boundary: D7,D9. |
| OFFLINE-022 | Given replay mode and live mode produce different normalized answers, when drift check runs, then scenario report marks contract drift instead of silently passing. | SS: drift report, answer diff, UI tree; Perf: diff timing; Deps: live-vs-replay contract fixture; Boundary: D4,D7. |
| OFFLINE-023 | Given offline voice mode has no local STT permission, when user starts listening, then text fallback appears and no audio buffer is retained. | SS: offline voice fallback, UI tree; Perf: fallback under 500 ms; Deps: local STT denied fixture; Boundary: D6,D7,D10. |
| OFFLINE-024 | Given gh-pages publishes offline/replay scenarios, when the report loads, then replay-ready, blocked, stale, and missing-cassette states are visually distinct. | SS: report rollup desktop and mobile, UI tree; Perf: report load and layout shift; Deps: gh-pages Playwright check; Boundary: process,D8. |

## Settings And Storage Expansion

| ID | Scenario | Evidence |
|---|---|---|
| SET-017 | Given provider credentials are cleared, when Settings reloads and Agent runs, then no stale credential state remains in UI or provider transport. | SS: clear action, provider status, agent error, UI tree; Perf: clear-to-state under 500 ms; Deps: credential store fixture; Boundary: D4,D7,D10. |
| SET-018 | Given provider reconnect is requested after key rotation, when validation succeeds, then the new connection state is shown and old key material is absent from logs. | SS: reconnect state, validation result, redaction log, UI tree; Perf: validation latency; Deps: provider validation cassette; Boundary: D7,D10. |
| SET-019 | Given notification permission changes outside the app, when Settings opens, then raw permission state refreshes without pretending app policy changed it. | SS: system-denied state, Settings row, UI tree; Perf: refresh under 500 ms; Deps: notification permission fixture; Boundary: D7. |
| SET-020 | Given Clear All Data runs after downloads and secrets exist, when reset completes, then onboarding appears and exported diagnostics prove secrets/downloads were removed. | SS: clear confirmation, onboarding, diagnostics proof, UI tree; Perf: reset time; Deps: seeded downloads and keychain fixture; Boundary: D4,D7,D10. |
| SET-021 | Given model catalogs contain deprecated, current, and unavailable models, when the picker opens, then each status is labeled and unavailable models cannot be selected. | SS: model picker statuses, UI tree; Perf: picker render under 500 ms; Deps: model catalog cassette; Boundary: D4,D7. |
| SET-022 | Given Dynamic Type and long provider names are enabled, when Settings provider rows render, then labels wrap or truncate without hiding validation status. | SS: provider settings at AX size, UI tree; Perf: layout settle under 250 ms; Deps: accessibility text size; Boundary: native render,D5. |
| SET-023 | Given diagnostics export includes relay, provider, and playback errors, when redaction scan runs, then private keys, tokens, raw audio, and private relay payloads are absent. | SS: export preview, redaction scan, UI tree; Perf: export scan time; Deps: diagnostic error fixture; Boundary: D10. |
| SET-024 | Given Whats New has many entries from concurrent changelog files, when the sheet opens, then entries are ordered by shipped_at and long copy remains readable. | SS: Whats New sheet, UI tree; Perf: open under 500 ms; Deps: changelog fixture set; Boundary: D4,D9,native render. |

## Social Expansion

| ID | Scenario | Evidence |
|---|---|---|
| SOC-017 | Given legacy contact-list data exists, when migration runs, then followed, muted, and blocked pubkeys map into one canonical Rust-owned projection. | SS: migration result, contact list, UI tree; Perf: migration timing; Deps: legacy contact fixture; Boundary: D4,D10. |
| SOC-018 | Given an unknown friend requests trust approval, when the approval sheet opens, then requested tools, data access, and deny/allow actions are explicit. | SS: approval sheet, tool list, UI tree; Perf: open under 500 ms; Deps: peer prompt fixture; Boundary: D7,D10. |
| SOC-019 | Given trust approval is denied, when the peer prompt response is generated, then no tool executes and denial state is signed only if policy allows. | SS: denial result, tool log, UI tree; Perf: denial under 500 ms; Deps: trust denial replay; Boundary: D7,D10. |
| SOC-020 | Given a peer-agent prompt asks for playback control, when approved, then the resulting play/seek action is visible and scoped to the requested episode. | SS: approval, action chip, player result, UI tree; Perf: action under 1 sec; Deps: peer prompt and playback replay; Boundary: D4,D7,D10. |
| SOC-021 | Given a screenshot feedback flow includes private account data, when annotation preview opens, then redaction warnings and crop tools appear before send. | SS: screenshot preview, redaction warning, UI tree; Perf: image encode time; Deps: screenshot fixture; Boundary: D7,D10. |
| SOC-022 | Given a blocked user publishes a comment, when comments load, then blocked content is hidden or clearly collapsed and no notification fires. | SS: collapsed blocked comment, notification log, UI tree; Perf: comments render under 1 sec; Deps: blocked user relay fixture; Boundary: D4,D10. |
| SOC-023 | Given iOS and Android decode the same social projection with trust state, when screens render, then direction, blocked, approved, and pending fields match. | SS: iOS/Android screenshots or decode logs, UI tree; Perf: decode time; Deps: social projection golden frame; Boundary: D4,D5. |
| SOC-024 | Given NIP-17 encrypted prompt variants arrive, when Pod0 handles normal, malformed, and wrong-recipient events, then only valid recipient events surface. | SS: valid prompt, malformed rejection, wrong-recipient log, UI tree; Perf: decrypt timing; Deps: NIP-17 variant replay; Boundary: D6,D7,D10. |
