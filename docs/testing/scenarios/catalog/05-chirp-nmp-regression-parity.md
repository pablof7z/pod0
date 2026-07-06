# BDD Catalog 05 - Chirp And NMP Regression Parity

These scenarios convert recent shipped Chirp fixes, Chirp validation evidence,
and current NMP doctrine drift into Pod0-specific validation targets. They are
not a claim that Pod0 already passes; each row must produce screenshots, UI
trees, logs, performance metrics, replay fixtures, and linked issue/PR evidence
before it can leave `incomplete`.

## Chirp Fix Parity

| ID | Scenario | Evidence |
|---|---|---|
| CHIRP-001 | Given every visible primary button is idle, when each button is tapped once, then every tap dispatches exactly one observable Rust action or native capability result. | SS: before/after button states; Perf: tap-to-state under 300 ms; Deps: UITestSeed plus tap audit log; Boundary: D4,D7,D8. |
| CHIRP-002 | Given an action is rejected by Rust immediately, when the user taps the control, then the UI shows the rejection state instead of staying optimistic or silent. | SS: rejection toast/banner; Perf: state under 500 ms; Deps: action rejection replay; Boundary: D6,D7. |
| CHIRP-003 | Given a long-running async action starts, when progress exceeds the first frame budget, then progress and cancel affordances remain visible and fresh. | SS: progress and cancel; Perf: first feedback under 150 ms; Deps: slow action replay; Boundary: D1,D6,D8. |
| CHIRP-004 | Given a long-running async action fails, when the terminal state arrives, then busy flags clear and retry is available without relaunch. | SS: failed terminal state; Perf: terminal render under 500 ms; Deps: terminal failure replay; Boundary: D6,D8. |
| CHIRP-005 | Given a queued publish later succeeds, when terminal lifecycle is observed, then queued, accepted, and published states are visible in order. | SS: lifecycle states; Perf: state cadence <= 60 Hz; Deps: relay lifecycle replay; Boundary: D3,D6,D8. |
| CHIRP-006 | Given a queued publish permanently fails, when terminal lifecycle is observed, then failure remains visible until user acknowledges or retries. | SS: permanent failure state; Perf: none; Deps: permanently failed publish replay; Boundary: D6,D7. |
| CHIRP-007 | Given terminal action state was already emitted, when the relevant screen opens later, then the final verdict is still visible and not evicted prematurely. | SS: open-later terminal verdict; Perf: none; Deps: observed terminal lifecycle fixture; Boundary: D4,D5. |
| CHIRP-008 | Given an action is canceled, when a stale provider or relay response arrives later, then the canceled flow does not mutate current UI. | SS: canceled state and ignored late result; Perf: no stale mutation; Deps: delayed response replay; Boundary: D4,D8,D9. |

## NMP Master Pin And Runtime Lifecycle

| ID | Scenario | Evidence |
|---|---|---|
| NMPM-001 | Given Pod0 builds against the current audited NMP master rev, when dependency pins are inspected, then all NMP crates use one coherent rev. | SS: manifest diff or audit log; Perf: none; Deps: Cargo metadata; Boundary: D0,D4. |
| NMPM-002 | Given persisted relay settings exist, when runtime starts through the normal app path, then relay config loads without relying on builder-only setup. | SS: relays after launch; Perf: launch-to-relays under 1 sec; Deps: persisted relay fixture; Boundary: D3,D4,D7. |
| NMPM-003 | Given relay lifecycle events burst during reconnect, when projections update, then emitted state is rate-limited and remains readable. | SS: diagnostics and metrics; Perf: emit cadence <= 60 Hz; Deps: reconnect replay; Boundary: D3,D8. |
| NMPM-004 | Given action-stage events burst during a publish batch, when the outbox screen renders, then UI coalesces updates without dropped terminal states. | SS: outbox batch; Perf: emit cadence <= 60 Hz; Deps: action-stage replay; Boundary: D6,D8. |
| NMPM-005 | Given `nmp.publish` dispatch rejects malformed input, when Pod0 calls publish, then the UI receives a Rust-owned failure state instead of hardcoded queued success. | SS: malformed publish failure; Perf: none; Deps: malformed publish replay; Boundary: D6,D7. |
| NMPM-006 | Given social share publishes a note, when relay routing is required, then no app-facing UI accepts or persists a manual relay URL. | SS: share flow; Perf: none; Deps: relay config fixture; Boundary: D3,D7. |
| NMPM-007 | Given relay config is removed, when app relaunches, then old relay rows do not reappear from stale native state. | SS: before remove and after relaunch; Perf: none; Deps: relay remove fixture; Boundary: D3,D4. |
| NMPM-008 | Given NMP codegen registries change, when CI runs, then action-builder and projection generated files are checked for drift. | SS: CI log; Perf: none; Deps: codegen drift fixture; Boundary: D0,D5. |

## NIP-05 And Discovery State

| ID | Scenario | Evidence |
|---|---|---|
| NIP05-001 | Given Add Show resolves a valid NIP-05 identifier, when lookup completes, then result is scoped to that exact identifier and session. | SS: scoped success state; Perf: lookup latency; Deps: NIP-05 success replay; Boundary: D4,D7. |
| NIP05-002 | Given Add Friend resolves a valid NIP-05 identifier, when lookup completes, then only the requested friend pubkey is selected. | SS: add friend result; Perf: lookup latency; Deps: NIP-05 success replay; Boundary: D4,D7. |
| NIP05-003 | Given a different profile resolves concurrently, when Add Show is waiting on NIP-05, then the unrelated profile cannot satisfy the request. | SS: concurrent lookup guard; Perf: none; Deps: concurrent lookup replay; Boundary: D4,D8. |
| NIP05-004 | Given NIP-05 lookup fails, when Rust returns the verdict, then Pod0 shows immediate failure copy and never waits for a local timeout only. | SS: failure copy; Perf: failure under 1 sec after verdict; Deps: NIP-05 failure replay; Boundary: D6,D7. |
| NIP05-005 | Given NIP-05 lookup is offline, when the network is unavailable, then the UI shows honest offline state without permanent spinner. | SS: offline lookup state; Perf: no spinner beyond budget; Deps: offline lookup replay; Boundary: D6,D8. |
| NIP05-006 | Given a malformed identifier is typed, when submitted, then validation blocks locally and no relay or HTTP request is made. | SS: validation error; Perf: none; Deps: malformed input fixture; Boundary: D6,D7. |
| NIP05-007 | Given lookup succeeds after prior failure, when the user retries, then prior terminal failure is replaced only by the new scoped result. | SS: retry success; Perf: none; Deps: retry lookup replay; Boundary: D4,D6. |
| NIP05-008 | Given lookup telemetry is inspected, when a failure is shown, then logs include redacted correlation IDs and no private keys or bearer tokens. | SS: log excerpt; Perf: none; Deps: redacted log fixture; Boundary: D10. |

## Projection Cache And Stale-State Guards

| ID | Scenario | Evidence |
|---|---|---|
| PROJ-001 | Given a typed projection frame has a stale revision, when ProjectionCache applies it, then prior newer UI state remains intact. | SS: decode log and UI state; Perf: no crash; Deps: stale revision frame fixture; Boundary: D4,D5. |
| PROJ-002 | Given a projection reset frame arrives, when the cache applies it, then stale rows from the prior session are removed before new rows render. | SS: reset before/after; Perf: cache apply time; Deps: projection reset fixture; Boundary: D4,D5. |
| PROJ-003 | Given a malformed sidecar payload arrives, when decoded, then the visible UI uses empty/error state and latches resync rather than crashing. | SS: error state and resync flag; Perf: no crash; Deps: malformed sidecar replay; Boundary: D5,D6. |
| PROJ-004 | Given active account changes, when account-scoped projections update, then stale prior-account data is not visible on any tab. | SS: account switch before/after; Perf: switch settle under 1 sec; Deps: two-account fixture; Boundary: D4,D5,D10. |
| PROJ-005 | Given a show row updates title and artwork, when projection revision advances, then the library row updates without app-side cache policy. | SS: row update; Perf: render under 500 ms; Deps: show update fixture; Boundary: D4,D5. |
| PROJ-006 | Given search results are cleared, when the search view closes, then stale results do not flash on the next open. | SS: close and reopen; Perf: no stale flash; Deps: search projection fixture; Boundary: D4,D5. |
| PROJ-007 | Given a view subscribes to a dynamic projection, when the view disappears, then Rust unregisters the read and no stale event log remains visible. | SS: unregister log; Perf: none; Deps: dynamic projection fixture; Boundary: D5,D8. |
| PROJ-008 | Given projection decoding is exercised on iOS, when generated decoders process golden frames, then Swift and Rust semantic fields match exactly. | SS: golden decode log; Perf: decode time; Deps: golden frame fixture; Boundary: D4,D5. |

## Offline, Relay, And Replay Honesty

| ID | Scenario | Evidence |
|---|---|---|
| OFFLINE-001 | Given Pod0 is offline and user creates a share/comment, when send is tapped, then pending state is visible and no false sent state appears. | SS: offline pending state; Perf: first feedback under 300 ms; Deps: offline relay replay; Boundary: D6,D7. |
| OFFLINE-002 | Given pending publish exists offline, when connectivity returns, then the outbox flushes and pending indicator disappears only after relay ACK. | SS: pending, reconnect, flushed; Perf: reconnect-to-flush timing; Deps: reconnect relay replay; Boundary: D3,D6,D8. |
| OFFLINE-003 | Given relay reconnect enters backoff, when diagnostics opens, then backoff state is explicit and does not look like success. | SS: diagnostics backoff; Perf: none; Deps: reconnect backoff replay; Boundary: D6,D7. |
| OFFLINE-004 | Given an uncached profile is opened offline, when lookup cannot resolve, then an honest unavailable state replaces permanent loading. | SS: unavailable profile; Perf: no indefinite spinner; Deps: offline profile replay; Boundary: D6,D8. |
| OFFLINE-005 | Given an uncached show is opened offline, when feed metadata is unavailable, then cached episode context remains usable and missing data is labeled. | SS: partial show state; Perf: none; Deps: offline show fixture; Boundary: D1,D6. |
| OFFLINE-006 | Given downloads are available offline, when playback starts, then player uses local audio and no provider network dependency blocks the flow. | SS: offline playback; Perf: start latency; Deps: downloaded episode fixture; Boundary: D4,D7. |
| OFFLINE-007 | Given provider cassette replay is enabled, when a provider request has no matching cassette, then validation fails closed instead of calling live network. | SS: cassette miss failure; Perf: none; Deps: cassette miss fixture; Boundary: D7,D10. |
| OFFLINE-008 | Given provider cassette replay is enabled, when agent, STT, TTS, embedding, and search requests replay, then every provider-backed UI can be explored without live credentials. | SS: replayed provider screens; Perf: replay latency budgets; Deps: provider cassette suite; Boundary: D7,D8,D10. |

## Visual, Accessibility, And Liquid Glass Parity

| ID | Scenario | Evidence |
|---|---|---|
| VIS-001 | Given Home, Library, Player, Agent, Settings, and Diagnostics screens render in light mode, when screenshots are compared, then visual hierarchy and tap targets remain coherent. | SS: light-mode screen set; Perf: screenshot capture timing; Deps: UITestSeed; Boundary: native render,D5. |
| VIS-002 | Given the same core screens render in dark mode, when screenshots are compared, then contrast, materials, icons, and state colors remain readable. | SS: dark-mode screen set; Perf: none; Deps: dark appearance seed; Boundary: native render,D5. |
| VIS-003 | Given Dynamic Type is set to an accessibility size, when core screens render, then no labels overlap and primary actions remain reachable. | SS: Dynamic Type screen set; Perf: layout settle; Deps: accessibility text size simulator setting; Boundary: native render,D5. |
| VIS-004 | Given Reduce Motion is enabled, when sheets, playback controls, and agent progress animate, then motion is reduced without removing state feedback. | SS: reduced motion states; Perf: none; Deps: reduce motion simulator setting; Boundary: native render,D8. |
| VIS-005 | Given Reduce Transparency is enabled, when Liquid Glass materials are present, then semantic foreground and fallback surfaces maintain legibility. | SS: reduced transparency comparison; Perf: none; Deps: reduce transparency simulator setting; Boundary: native render. |
| VIS-006 | Given a long podcast title, author, and episode title render, when each major list/detail screen opens, then text truncates deliberately and never overlaps controls. | SS: long text screens; Perf: none; Deps: long metadata fixture; Boundary: native render,D5. |
| VIS-007 | Given a modal or sheet is presented, when scrolled behind navigation chrome, then content does not ghost through bars or tab regions. | SS: scrolled sheet/nav state; Perf: none; Deps: long settings fixture; Boundary: native render. |
| VIS-008 | Given screenshot critique is published, when gh-pages renders each visual scenario page, then every screenshot has alt text, dimensions, UI critique, UX critique, and issue links for defects. | SS: report page screenshot; Perf: generated page load; Deps: gh-pages Playwright check; Boundary: process. |

## Performance And D8 No-Polling Gates

| ID | Scenario | Evidence |
|---|---|---|
| D8-001 | Given haptic feedback sequences run, when success, warning, undo, and bulk feedback are triggered, then no Task.sleep polling path is used. | SS: haptic test log; Perf: no delayed busy task; Deps: haptics harness; Boundary: D8. |
| D8-002 | Given press feedback is shown after copy, when feedback clears, then it uses event-driven state rather than sleeping before mutation. | SS: copy feedback states; Perf: feedback lifetime metric; Deps: copy action harness; Boundary: D8. |
| D8-003 | Given CarPlay starts before the kernel is ready, when readiness arrives, then CarPlay updates via callback/state instead of fixed sleep. | SS: CarPlay startup log; Perf: startup wait budget; Deps: CarPlay harness; Boundary: D7,D8. |
| D8-004 | Given a 1000-show library snapshot is decoded, when the Library screen appears, then decode, commit, and render stay off the main hot path. | SS: perf trace; Perf: decode and frame hitch budget; Deps: large library fixture; Boundary: D5,D8. |
| D8-005 | Given playback reports arrive rapidly, when projections update, then player UI cadence is coalesced and never exceeds 60 Hz. | SS: player metrics; Perf: update rate <= 60 Hz; Deps: audio report replay; Boundary: D8. |
| D8-006 | Given provider replay returns instantly, when agent response renders, then the app still shows useful progress and avoids layout thrash. | SS: agent replay render; Perf: render under 500 ms; Deps: provider cassette suite; Boundary: D1,D8. |
| D8-007 | Given a generated report page contains screenshot galleries, when desktop and mobile viewports load it, then images render without layout shift or text overlap. | SS: Playwright desktop/mobile screenshots; Perf: page load and image decode timing; Deps: gh-pages static report; Boundary: process. |
| D8-008 | Given architecture scanner runs on Pod0, when D8 findings are present, then each finding maps to an issue, scenario page, and validation owner before release. | SS: scanner summary; Perf: none; Deps: nmp architecture scanner output; Boundary: D8,process. |
