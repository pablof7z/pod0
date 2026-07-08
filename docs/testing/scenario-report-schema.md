# Pod0 Scenario Report Schema And Site Architecture

This document defines the static site shape and generated JSON contract for
per-scenario pages on the Pod0 validation GitHub Pages site. The companion
machine-readable schema is [scenario-report.schema.json](scenario-report.schema.json).

## Static Site Shape

The public site should be generated from versioned JSON records, not from hand
edited HTML. `gh-pages` remains the deployment branch; source records and docs
live on normal feature branches under `docs/testing/`.

Recommended generated layout:

```text
/
  index.html
  scenarios/
    index.html
    a-onboarding-identity/
      index.html
    d-playback/
      index.html
    d1-play-pause-resume/
      index.html
      data.json
    m3-full-pipeline-capstone/
      index.html
      data.json
  data/
    scenarios.json
    rollups.json
    tags.json
    issues.json
    schema-version.json
  tags/
    index.html
    cassette-required/
      index.html
  issues/
    index.html
  rollups/
    verdict/incomplete/index.html
    provider/blocked/index.html
    nmp/d7/index.html
    performance/index.html
  assets/
    scenarios/
      d1-play-pause-resume/
        20260705T181200Z-step-01-now-playing.png
        20260705T181215Z-step-02-mini-player.png
        20260705T181230Z-trace-performance.json
        20260705T181240Z-ui-tree.json
    cassettes/
      openrouter-whisper/
      ollama-agent/
    product/
```

## URL Scheme

- Home: `/`
- Scenario index: `/scenarios/`
- Category index: `/scenarios/<category-slug>/`
- Scenario page: `/scenarios/<scenario-slug>/`
- Scenario data: `/scenarios/<scenario-slug>/data.json`
- Global scenario index JSON: `/data/scenarios.json`
- Rollups: `/data/rollups.json`
- Tags and filters: `/data/tags.json`
- Issue/PR summary: `/data/issues.json`
- Tag rollup pages: `/tags/<tag-slug>/`
- Defect rollup page: `/issues/`
- Provider, verdict, NMP-rule, and performance rollups:
  `/rollups/<rollup-kind>/<value>/`

Scenario slugs must be stable and derived from the canonical scenario ID and
title, for example `d1-play-pause-resume`. If a title changes, keep the existing
slug and update the display title.

## Index And Category Pages

The generator must produce:

- A global scenario table with status, grouped scores, category, tags, freshness,
  required evidence state, issue count, fix PR count, and last run.
- Category pages for A-M and future catalog groups.
- Filter controls for verdict, category, tag, owner, device, OS version,
  provider mode, cassette coverage, issue severity, evidence freshness, and NMP
  doctrine coverage.
- Sort controls for scenario order, risk, recency, score, open defects, and
  missing evidence.
- A compact rollup band that separates `pass`, `pass_with_issues`, `fail`,
  `blocked`, and `incomplete`.

Rollups must be computed from page JSON. Do not maintain totals by hand.

## Page Navigation

Every scenario page must include:

- Previous/next scenario links in canonical scenario order.
- Category index link.
- "Related scenarios" links from tags and explicit dependency metadata.
- Defect issue links and fix PR links.
- Source runbook/catalog link back to `docs/testing/scenarios/`.
- Revalidation links from a defect page back to the scenario evidence that closed
  the issue.
- Provider replay coverage that names any mapped cassette fixture IDs and links
  the redacted fixture artifacts, or explicitly states that provider-backed
  validation is blocked by missing replay coverage.

## JSON Record Overview

Each scenario page is generated from one `ScenarioReport` JSON object:

| Field | Purpose |
| --- | --- |
| `schema_version` | Version of this contract. Current value: `1.1.0`. |
| `scenario` | Stable scenario identity, source paths, BDD text, category, tags, dependencies. |
| `run` | Build, executor, device/OS matrix, environment, fixtures, provider/cassette mode. |
| `page` | Canonical URLs, generated timestamps, nav links, asset base. |
| `product_context` | Persona, job-to-be-done, user value, acceptance criteria, platform expectations, and scenario cluster. |
| `flow_steps` | Ordered step-by-step flow with expected state and required evidence per step. |
| `execution` | Attempts, retries, branches, commands, tools, rerun notes, and branch coverage. |
| `review_grounding` | Exact skill-search command, selected/considered skills, and how those skills shape the template. |
| `launch_assessment` | Launch readiness, risk class, evidence quality, accessibility status, regression coverage, dependency posture, owner/status, blocking gates, issue refs, and whole-product judgment. |
| `sections` | Required narrative sections from the page template, including provenance, before/after deltas, revalidation status, and owner/status. |
| `dimension_scores` | Per-dimension scores and evidence references. |
| `group_scores` | Functional/product/engineering/follow-through grouped scores. |
| `quality_review` | Structured UI, UX, performance, accessibility, reliability, privacy/security, content, controls, offline/resume, and observability checks. |
| `coherence` | Individual scenario judgment plus related-scenario group-level product coherence judgment. |
| `readiness` | Release gates, blockers, owners, and ship-gate status. |
| `evidence` | Artifact registry for screenshots, videos, logs, UI trees, metrics, cassettes, relay JSON, generated-page viewport checks, source docs, plus visible placeholders for missing required evidence. |
| `metrics` | Measured values, budgets, units, collection methods, and status. |
| `instrumentation_gaps` | Missing instrumentation/evidence with severity, owner, and affected dimensions. |
| `risks` | Risk register with severity, priority, status, affected dimensions, and mitigation. |
| `issues` | Defects, severities, issue links, fix PRs, and revalidation state. |
| `next_actions` | Ordered follow-up work with owner/link/status. |

The generator treats the structured fields above, `sections`, `dimension_scores`,
and `evidence` as the source of truth for the visible page. Summary JSON is
derived from these fields.

## Required Dimension IDs

The schema requires these dimension IDs:

```text
persona_acceptance
flow
attempted_test
scenario_setup
execution_attempts
expected_behavior
actual_result
artifacts
evidence_provenance
review_skill_grounding
ui_polish
ux_polish
performance
accessibility_dynamic_type
liquid_glass_ios_primitives
error_recovery
privacy_security
nmp_architecture
product_coherence
product_cluster_coherence
before_after_deltas
reliability_flakiness
regression_risk
defects_issues_filed
revalidation_status
risks_follow_up
instrumentation_gaps
localization_content_quality
controls_gestures_audio
offline_resume_behavior
readiness_gates
verdict
next_actions
owner_status
data_integrity_state_sync
navigation_state_restoration
device_viewport_coverage
media_session_background_continuity
cross_screen_continuity
states_resilience
touch_ergonomics
motion_haptics
information_architecture
content_hierarchy
observability
analytics_privacy_boundaries
replayability_cassette_provenance
evidence_confidence
device_os_matrix
sister_app_nmp_chirp_comparison
```

`states_resilience` covers empty, loading, denied, unavailable, offline, retry,
and recovery states.
`device_viewport_coverage` covers both app-device coverage and generated
GitHub Pages desktop/mobile viewport smoke checks.
`media_session_background_continuity` is `N/A` only when the scenario has no
podcast playback, queue, route, lock-screen, or background continuity exposure.

## Evidence And Asset Rules

Artifact IDs are stable within a scenario record. Paths are relative to the
generated site root.

Screenshots:

- Store under `assets/scenarios/<scenario-slug>/`.
- Name with UTC timestamp, step ID, and short purpose:
  `20260705T181200Z-step-03-error-banner.png`.
- Include `alt`, `caption`, `step_id`, `device`, `os_version`,
  `captured_at`, `sha256`, `width`, and `height`.
- Do not reuse temp paths from simulator captures in published JSON.

Videos:

- Store beside screenshots and link at least one representative still image.
- Include duration, interaction proven, and Reduce Motion note when relevant.

Metrics:

- Store raw traces or summarized JSON beside scenario assets.
- Each metric must name value, unit, budget, status, collection method, and
  artifact reference.

Cassettes:

- Store or link redacted cassette fixtures by provider and scenario.
- Include provider, model, request class, redaction hash, recorded-at timestamp,
  and whether the scenario used live, replay, or mixed mode.
- Scenario pages must surface mapped cassette fixtures directly from
  `tests/fixtures/provider_cassettes/`, copy them to
  `assets/cassettes/provider_cassettes/`, and register them as `cassette`
  artifacts so provider-backed flows are visibly runnable without live
  credentials.
- A scenario with mapped replay fixtures may mark the cassette fixture itself as
  present, but it still remains incomplete until an execution attempt proves the
  app used replay mode and captured screenshots, UI tree, logs, metrics, and
  UX/UI critique.

Missing evidence:

- `evidence.required_kinds` names every evidence class required by the scenario.
- `evidence.missing` lists every absent required kind, why it matters, and which
  dimensions it blocks.
- `instrumentation_gaps` repeats missing evidence as an owner/severity queue so
  rollups can find gaps without parsing section prose.
- `report_viewport` evidence records desktop/mobile smoke checks for the
  generated scenario page itself so the public validation page is also
  reviewable.
- `evidence_provenance` records capture origin, source commit, branch, device/OS,
  redaction, freshness, and live/replay/generated/copied status.
- `before_after_deltas` records the state before an action, the action itself,
  the state after it, the intended delta, and any unexpected regression.
- `revalidation_status` records fix PRs, revalidation run IDs, rerun commits,
  affected dimensions, and still-open gaps.
- `owner_status` makes owners and statuses explicit across blockers, gates,
  risks, issues, and follow-up actions.

Security:

- Published artifacts must be redacted before generation.
- Secret-bearing artifacts are forbidden on `gh-pages`.
- Redaction metadata must say what was removed without exposing the secret.

Schema enforcement:

- `quality_review` must contain the canonical UI, UX, performance,
  accessibility, reliability, privacy/security, content/localization,
  controls/gestures, offline/resume, and observability areas.
- `coherence.cluster` must carry stable cluster identity and related scenario
  IDs.
- Screenshot artifacts must include alt text, caption, step ID, capture time,
  device, OS version, SHA-256, width/height, and required/elective status.

## Summary Rollups

`/data/rollups.json` should include:

- Counts by verdict, category, tag, device, OS, provider mode, evidence state,
  readiness, product cluster, group-coherence status, and instrumentation gap ID.
- Computed average score by dimension and group, ignoring only `N/A` values.
- Missing evidence counts by evidence type.
- Issue counts by severity, open issue counts by severity, and scenario sources
  for each rollup.
- Revalidation state after fix PR merge.
- Flake/stale-build warnings.
- NMP doctrine coverage, including D0-D10 touched and failed counts.

Rollups must link back to the page JSON that produced each count.

## Issue And PR Back-Links

Every issue entry must include:

- GitHub issue URL and number.
- Severity: `blocker`, `major`, `minor`, or `polish`.
- Affected dimension IDs.
- First-seen scenario run ID and source commit.
- Owner or `unowned`.
- Fix PR URL when available.
- Revalidation scenario run ID when fixed.

A page with any actionable defect that lacks an issue URL is `incomplete`.

## Product-Specific Data Requirements

Podcast validation records should capture these domain facts when relevant:

- Episode/show identifiers, feed URL class, Nostr event IDs, relay URLs, and
  relay response JSON. Redact private keys and tokens.
- Audio state: route, duration, position, speed, queue state, background/lock
  screen state, and remote command behavior.
- Data/state integrity: persisted store, Rust projections, native render state,
  export/import payloads, and before/after deltas agree after the action.
- Transcript state: publisher transcript, generated transcript, provider,
  segment seek behavior, and missing transcript fallback.
- Agent state: provider/model, cassette ID, grounded source references,
  conversation continuity, voice/text mode, and tool calls.
- Highlight/publishing state: NIP-84/NIP-F4 event kinds, tag conformance,
  signatures, author claims, and negative checks.
- NMP boundary notes: single writer, action/event, projection/FFI shape,
  capability-report path, bounded cadence, and privacy posture.

## Generator Failure Rules

The generator must fail or emit `incomplete` when:

- JSON does not validate against `scenario-report.schema.json`.
- Required section text is empty.
- A score is `3` or `4` without evidence references.
- A required artifact reference is missing from `evidence.artifacts`.
- A referenced file is absent from the generated output.
- `verdict.overall` conflicts with score gates.
- A defect is present without an issue URL.
- Metrics required by the scenario type are missing.
- Screenshots lack alt text, captions, or redaction metadata.

## Reviewer Workflow

1. Open the scenario page, not the rollup only.
2. Read the header to confirm commit, device/OS, fixture, and freshness.
3. Compare expected behavior with actual result and artifacts.
4. Inspect UI/UX, performance, accessibility, privacy, and NMP dimensions.
5. Verify every defect is linked to an issue and every fix has revalidation.
6. Approve only if the visible verdict follows the page's own scores and
   evidence.

The page template is defined in
[scenario-report-page-template.md](scenario-report-page-template.md).
