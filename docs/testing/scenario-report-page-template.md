# Pod0 Per-Scenario Report Page Template

This is the canonical page contract for generated Pod0 validation report pages.
It applies to every BDD scenario page on the public validation site. A page that
omits evidence must say so explicitly and must not present the scenario as
passed.

## Page Goals

- Make one scenario independently reviewable without opening the giant report.
- Preserve enough evidence to reproduce the run, inspect the product quality,
  and file defects without re-running the simulator.
- Score each validation dimension, then combine the dimensions into a cohesive
  verdict about the user experience and architecture.
- Scale to hundreds of static pages generated from JSON records.

## Score Scale

Each dimension uses the same score:

| Score | Meaning | Gate |
| --- | --- | --- |
| `0` | Not assessed or no evidence. | Overall verdict is `incomplete`. |
| `1` | Blocking failure or severe product break. | Overall verdict is `fail`. |
| `2` | Partial, rough, or fragile behavior. | Requires linked issue unless intentionally accepted. |
| `3` | Meets current Pod0 baseline. | Needs current evidence. |
| `4` | Premium 2026 iOS podcast-app quality. | Needs evidence plus critique explaining why it is excellent. |
| `N/A` | Truly not applicable. | Requires rationale and reviewer acceptance. |

No dimension may score above `2` without current evidence. "Looks fine" is not
evidence. Screenshots, UI trees, videos, metric traces, cassette IDs, relay
JSON, command output, or linked source documents are evidence.

## Verdict Model

The page shows dimension scores and five grouped scores:

| Group | Dimensions |
| --- | --- |
| Functional correctness | Flow, attempted test, setup, expected behavior, actual result, error/recovery behavior, regression risk. |
| Evidence and reproducibility | Artifacts, evidence provenance, review skill grounding, replayability/cassette provenance, device/OS matrix, evidence confidence, defects/issues filed. |
| Product experience | UI polish, UX polish, Liquid Glass/iOS primitive integration, cross-screen continuity, before/after deltas, empty/loading/offline states, touch ergonomics, motion/haptics, information architecture, content hierarchy, product coherence. |
| Engineering quality | Performance metrics, accessibility/dynamic type, privacy/security, NMP architecture/cohesiveness, observability, analytics/privacy boundaries. |
| Follow-through | Defects/issues filed, revalidation status, verdict, next actions, issue/PR back-links, owner/status. |

Overall verdict is one of:

- `pass`: no score below `3`, no unresolved blocking/major issue, all required
  evidence present.
- `pass_with_issues`: all acceptance criteria pass, but at least one score is
  `2` or a nonblocking issue remains linked.
- `fail`: scenario behavior violates expected behavior or has any score `1`.
- `blocked`: prerequisites prevented execution. The page must state exactly what
  blocked the run and what would unblock it.
- `incomplete`: required evidence, metrics, critique, or issue filing is missing.

## Required Header

Every page starts with:

- Scenario ID, title, category, tags, and source runbook/catalog path.
- Canonical URL and generated-at timestamp.
- Source commit, branch, app build, schema version, and report generator version.
- Executor, simulator/device, OS version, locale, appearance, network condition,
  provider mode, and launch arguments.
- Previous/next scenario links, category index link, and summary rollup link.
- Verdict badge plus grouped score summary.

## Required Structured Surfaces

The visible sections are backed by first-class JSON so the site can scale to
hundreds of pages without prose scraping:

- `product_context`: persona, job-to-be-done, user value, acceptance criteria,
  platform expectations, and scenario cluster.
- `flow_steps`: ordered precondition/action/result/exit steps with required
  evidence per step.
- `execution`: every attempt, retry, command, tool, branch, and rerun note.
- `review_grounding`: exact `npx skills search` command, loaded skill names,
  considered skills, and how the skills shaped the template.
- `launch_assessment`: launch readiness, risk class, evidence quality,
  accessibility status, regression coverage, dependency posture, owner/status,
  blocking gates, issue refs, and whole-product judgment.
- `quality_review`: UI, UX, performance, accessibility, reliability,
  privacy/security, content/localization, controls/gestures, offline/resume,
  and observability review areas.
- `coherence`: both individual scenario judgment and related-scenario
  group-level product coherence judgment.
- `readiness`: release gates and blockers.
- `evidence.missing`, `evidence_provenance`, `before_after_deltas`,
  `evidence.placeholders`, `revalidation_status`, `owner_status`,
  `instrumentation_gaps`, and `risks`: explicit blockers, affected dimensions,
  owners, mitigations, freshness, and follow-through. Missing screenshots,
  metrics, UI trees, cassettes, logs, command output, and accessibility audits
  must render as visible placeholders.

The required search command for this template is
`npx skills search "Liquid Glass iOS mobile UI UX polish accessibility frontend design"`. The
selected grounding is:

- `heyman333/atelier-ui@ios-glass-ui-designer`, loaded after `npx skills
  search`, for iOS-native hierarchy, restrained glass/material use, system
  typography, semantic foreground styles, safe areas, and native sheets,
  navigation, and accessibility fallbacks.
- `phazurlabs/ux-ui-mastery@Mobile UX Design`, loaded after `npx skills search`,
  for mobile-first user goals, thumb-zone reachability, 44 pt/48 dp touch
  targets, interruption/resume behavior, platform navigation conventions, and
  performance-as-UX budgets.

The generated HTML must front-load the deep-review areas a reviewer expects:
what was attempted/test intent, flow, data/control-plane setup, result, evidence,
UI polish, UX polish, performance/latency, navigation/orientation, motion/haptic
quality, product-flow cohesiveness, group/coherent-product judgment, risk
severity, and validation confidence.

## Required Sections

| Section | Must Answer | Required Evidence | Score Gate |
| --- | --- | --- | --- |
| Launch readiness summary | Is this scenario launch-ready as an individual flow and as part of the whole product? | Readiness gates, risk class, evidence quality, accessibility/regression status, dependency posture, issue refs, owner/status. | Missing evidence or incomplete cluster judgment keeps launch readiness below pass. |
| Persona/job/acceptance | Who is this flow for, what job must it complete, and what acceptance criteria define success? | Scenario BDD, cluster link, platform expectations. | `3+` requires user value and acceptance criteria tied to evidence. |
| Flow | What user journey did this scenario validate, and where does it sit in the app? | Step list, source scenario link, navigation breadcrumbs. | `3+` requires a complete path from launch state to exit state. |
| Attempted test | What exactly was executed, by whom/what, and with which tools? | Commands, simulator/tool session IDs, automation/manual notes. | Missing commands or tool notes force `incomplete`. |
| Scenario setup | What fixture, account, provider, network, relay, and data state was present? | Fixture manifest, launch args, cassettes, seeded data, relay/provider status. | `3+` requires deterministic or clearly documented live inputs. |
| Execution attempts | Which attempts, retries, branches, and reruns occurred? | Attempt records, commands, retry causes, branch IDs, stale evidence notes. | Hidden retries or branch mutations force `incomplete`. |
| Expected behavior | What should a correct Pod0 implementation do? | Given/When/Then text and acceptance criteria. | `3+` requires user-visible and architecture expectations. |
| Actual result | What happened step by step? | Screenshot/UI tree/log per meaningful step. | Any unobserved critical step caps at `2`. |
| Artifacts/screenshots/video | What visual and raw evidence supports the result? | Screenshot gallery, videos, UI trees, logs, metric traces, SHA/path/URL. | Missing required screenshot forces `incomplete`. |
| Evidence provenance | Where did each artifact come from, and can it be trusted? | Capture command/tool, source commit, branch, device/OS, SHA/path, redaction state, freshness, live/replay/generated/copied marker. | Unknown provenance caps affected evidence dimensions at `2`. |
| Review skill grounding | Which external review skills, platform rubrics, or design doctrines grounded the page's observations? | `npx skills search` terms, loaded skill names/versions, rubric notes, and reviewer coverage. | Missing skill grounding forces `incomplete`; UI/UX scores above `2` require relevant product/design/iOS skills. |
| UI polish report | Does the screen look finished and platform-native? | Annotated screenshots for layout, spacing, typography, color, symbols, control states. | `3+` requires critique, not just a screenshot. |
| UX polish report | Does the flow feel clear, focused, and recoverable? | Notes on task clarity, user effort, feedback, interruption/resume, cognitive load. | `3+` requires analysis of the user's goal, not component-by-component notes only. |
| Performance metrics | Did the scenario stay responsive? | Launch/tap latency, screen-settle time, scroll FPS/hitches, memory, CPU, audio/LLM/network latency when relevant. | Missing relevant metrics forces `incomplete`; regressions force `fail` or `pass_with_issues`. |
| Accessibility/dynamic type | Does it work for assistive settings? | VoiceOver/UI tree labels, Dynamic Type screenshots, contrast, Reduce Motion/Transparency notes, touch target checks. | `3+` requires at least one accessibility evidence artifact. |
| Liquid Glass/iOS primitive integration | Does the UI use 2026 iOS primitives correctly? | Evidence for standard bars/sheets/tabs/toolbars, semantic colors, system fonts, glass restraint, Reduce Transparency/Motion behavior. | `4` requires glass as functional chrome, not decorative content material. |
| Error/recovery behavior | What happens on failure and retry? | Error screenshots, retry logs, offline/provider/relay failure evidence. | Silent failure, crash, or permanent spinner is `1`. |
| Privacy/security | Are secrets, signing, private events, and permissions handled safely? | Redaction review, permission prompt screenshots, key/relay behavior, log scan notes. | Leaked secret or public fallback for private data is `1`. |
| NMP architecture/cohesiveness | Does behavior respect Rust-core/thin-shell ownership? | D0-D10 notes, FFI snapshot/projection evidence, capability-report evidence, source links when inspected. | Native policy, duplicate state, unbounded FFI, polling, or privacy fallback is `1`. |
| Product coherence in context | Does this scenario fit Pod0's product promise and adjacent flows? | Cross-links to related scenarios, before/after screenshots, comparison to expected Pod0 mental model. | `3+` requires continuity with surrounding product surfaces. |
| Product cluster coherence | Do related scenarios agree as a product group? | Cluster rollup, related scenario links, shared theme/defect notes. | Individual UI/UX scores cannot pass if group-level coherence is contradictory. |
| Before/after deltas | What user-visible state changed, and what should not have changed? | Before screenshot/state, action, after screenshot/state, expected delta, unexpected regression notes. | Missing delta evidence caps product-experience scores at `2`. |
| Reliability/flakiness | How stable is the behavior across reruns and retries? | Rerun count, flake notes, retry causes, stale build checks, deterministic replay evidence. | Flaky or stale evidence caps at `2`. |
| Regression risk | What could this break, and what should be rerun? | Related scenarios, impacted modules, prior bugs, merge-gate references. | Missing risk notes cap at `2`. |
| Defects/issues filed | Were all imperfections tracked? | GitHub issue links, severity, owner, fix PR link if available. | Any unfiled actionable defect forces `incomplete`. |
| Revalidation status | Did fixes actually close the scenario risk? | Fix PR link, revalidation run ID, rerun commit, affected dimensions, still-open gaps. | Fixed issue without revalidation cannot count as resolved. |
| Risks/follow-up | Which risks remain, who owns them, and what PR/issue/revalidation follows? | Risk records, priorities, issue/PR links, recommended mitigation. | Unowned blocker or major risk prevents pass. |
| Instrumentation gaps | Which evidence is missing and which dimensions are blocked by it? | Missing-evidence inventory, gap severity, owner, affected dimensions. | Hidden gaps force `incomplete`. |
| Localization/content quality | Does content survive locale, long text, transcript, and empty/error copy cases? | Locale screenshots, copy review, truncation notes, transcript/metadata checks. | Broken critical content caps at `2`. |
| Controls/gestures/audio | Are buttons, gestures, sheets, audio controls, haptics, and alternatives correct? | Control audit, gesture alternatives, haptic/audio notes, reachability screenshots. | Gesture-only critical action or inaccessible audio control caps at `2`. |
| Offline/resume behavior | Does the flow recover across offline state, relaunch, interruption, and stale data? | Offline/relaunch/background evidence, retry/cancel logs. | Permanent spinner or lost state is `1`. |
| Readiness gates | Which release gates are open, blocked, or done? | Gate table, blockers, owners, linked evidence. | Any blocked release gate keeps verdict below pass. |
| Verdict | What is the scenario's status and why? | Grouped score summary and concise rationale. | Must match score gates mechanically. |
| Next actions | What should happen next? | Ordered action list with owners/links. | Blocking follow-up without owner/link caps at `2`. |
| Owner/status | Who owns the page, defects, blockers, gates, and revalidation? | Owner/status on blockers, gates, risks, issues, and next actions. | Unowned blocker prevents pass. |
| Data integrity/state sync | Did persisted data, Rust projections, native render state, and exported/imported records agree? | Projection snapshots, persistence/export/import checks, before/after state, logs. | Divergent user-visible or persisted state is `1`. |
| Navigation state/restoration | Does tab, sheet, detail path, back stack, deep-link, and relaunch restoration state remain correct? | Navigation screenshots, UI tree, route/deep-link logs, relaunch evidence. | Stale or contradictory navigation state caps at `2`; lost critical state is `1`. |
| Device/viewport coverage | Which app devices/settings and generated-site viewports prove the result? | iPhone/device matrix plus desktop/mobile report-page screenshots or Playwright checks. | Missing generated-page viewport smoke keeps report-page readiness incomplete. |
| Media session/background continuity | For podcast-facing flows, do audio route, queue, mini-player/full-player, lock-screen/background, interruption, and remote command state remain coherent? | Audio/session logs, player screenshots, lock/background notes, remote-command evidence. | Broken background or contradictory player state is `1`. |

## 2026 Product-Quality Sections

These sections are also required because Pod0 is a premium mobile podcast app,
not a unit-test dashboard.

| Section | Must Answer | Required Evidence | Score Gate |
| --- | --- | --- | --- |
| Cross-screen continuity | Do navigation, mini-player, queue, transcript, agent, and settings state remain coherent across screens? | Screenshots before/after navigation, state snapshots, back-stack notes. | Lost context or contradictory state is `1`. |
| Empty/loading/offline states | Are waiting, empty, denied, unavailable, and offline states intentional? | Screenshots for each state reached or explicit `N/A` rationale. | Spinner without progress or recovery is `1`. |
| Touch ergonomics | Are primary controls reachable, large enough, and separated? | Touch target audit, one-handed notes, gesture alternatives. | Critical controls under 44x44 pt or gesture-only actions cap at `2`. |
| Motion/haptics | Does motion clarify state without slowing the user? | Video or notes for transitions, haptic expectations, Reduce Motion behavior. | Gratuitous, blocking, or inaccessible motion caps at `2`. |
| Information architecture | Does the user know where they are and where to go next? | Breadcrumbs, titles, tab/toolbar analysis, related flow links. | Ambiguous location or hidden primary action caps at `2`. |
| Content hierarchy | Is podcast content, episode metadata, transcript text, and agent output prioritized correctly? | Annotated screenshots, truncation notes, Dynamic Type checks. | Important content hidden by chrome or decoration caps at `2`. |
| Observability | Can engineers diagnose the run without reproducing it? | Structured logs, metric IDs, provider/request IDs, redacted traces. | Missing diagnostics for a failure caps at `2`. |
| Analytics/privacy boundaries | Are measurable events useful without collecting excess data? | Event names, redaction policy, opt-out/permission notes, absence of secrets. | PII or secrets in analytics/logs is `1`. |
| Replayability/cassette provenance | Can provider-backed behavior be replayed? | Cassette IDs, provider/model, redaction hash, live-vs-replay marker. | Live-only provider evidence caps at `2` unless scenario is explicitly live validation. |
| Evidence confidence | How trustworthy is the result? | Freshness, build match, rerun count, flake notes, stale evidence warnings. | Stale build or single flaky run caps at `2`. |
| Device/OS matrix | Which devices and settings prove the result? | Device/OS table, locale, appearance, Dynamic Type, network condition. | iPhone-only smoke may pass only if the page labels matrix coverage as limited. |
| Sister app/NMP/chirp comparison | Does Pod0 match shared NMP doctrine and known sister-app fixes? | Links to NMP/chirp expectations, applicable fix notes, divergence rationale. | Known applicable fix missing and untracked forces `incomplete`. |

## Evidence Rules

- Every meaningful user-visible step needs a screenshot. Host-only checks need
  command output or raw JSON instead.
- Every screenshot must have a stable asset path, short caption, step ID, capture
  timestamp, device/OS, alt text, and intrinsic width/height.
- Every video must name the interaction it proves and link the related still
  screenshot.
- Every provider, relay, STT, TTS, LLM, search, or network dependency needs a
  cassette ID, live-run justification, or blocked rationale.
- Every metric must include value, unit, budget, status, collection method, and
  artifact reference.
- Every defect must link to a GitHub issue before the page can leave
  `incomplete`, even when the scenario otherwise passes.
- Redacted evidence must say what was redacted and why. Secret-bearing artifacts
  must never be published.

## Reviewer Acceptance Criteria

A reviewer may approve a scenario page only when:

- All required sections are present.
- All non-`N/A` dimensions have scores and evidence references.
- The verdict follows the scoring gates above.
- Screenshots cover the happy path plus relevant empty/loading/error/offline
  states.
- Performance and accessibility evidence exists for every scenario where the
  user would feel latency, scrolling, media playback, text scaling, or touch
  precision.
- UI/UX critique contains concrete product observations and not generic praise.
- NMP/RMP notes identify state ownership, FFI boundary, capability behavior,
  privacy posture, and bounded reactivity when relevant.
- Every actionable issue is filed and back-linked.

## Generator Acceptance Criteria

The static report generator must fail the build or mark a page `incomplete` when:

- A required section is missing.
- A required score is missing or outside the allowed scale.
- A score is `3+` without evidence references.
- A required screenshot, metric, cassette, or issue link is missing.
- A linked artifact path does not exist in the generated site.
- The page verdict conflicts with dimension or grouped scores.
- Asset alt text, captions, or redaction metadata are missing.
- Summary rollups disagree with per-page JSON.

See [scenario-report-schema.md](scenario-report-schema.md) for the data model,
URL scheme, asset layout, and JSON Schema.
