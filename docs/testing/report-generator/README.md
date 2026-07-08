# Pod0 Scenario Report Generator

This generator turns the BDD catalog in `docs/testing/scenarios/catalog/` into a
static report site with one page and one `data.json` record per scenario.

Run it from the repository root:

```sh
python3 docs/testing/report-generator/generate_scenario_report.py \
  --out build/pod0-scenario-report
```

Evidence-backed scenario records live under `docs/testing/evidence/`. The
generator reads `docs/testing/evidence/scenario-records/*.json` as overlays on
top of catalog-derived scaffold records, copies
`docs/testing/evidence/assets/` into the generated `assets/` directory, and
keeps untouched catalog rows scaffolded as `incomplete`.

The generator is intentionally strict:

- The catalog `INDEX.md` total must match the parsed row count.
- Every catalog row must parse as `Given ..., when ..., then ...`.
- Every generated scenario record must contain all required sections and scores
  from `docs/testing/scenario-report.schema.json`.
- Every generated scenario page is `incomplete` until real screenshots, UI
  trees, metrics, cassettes, accessibility evidence, UI/UX critique, and issue
  links are attached.
- Generated scaffold pages carry GitHub issue-backed validation blockers for
  the known missing evidence, accessibility, performance, cassette, NMP/chirp,
  and D8 work. These blockers do not count as observed run evidence by
  themselves.
- Artifact paths in generated JSON must exist in the generated site.
- Rollup counts must agree with per-scenario records.

The generated site includes:

- `scenarios/<scenario-id>/index.html`
- `scenarios/<scenario-id>/data.json`
- `scenarios/index.html`
- category index pages under `scenarios/<category>/`
- tag pages under `tags/<tag>/`
- rollups for verdict, provider mode, NMP boundary, performance requirements,
  and issues
- `next-wave/index.html` plus `data/next-wave.json` for the planned
  screenshot-backed execution wave
- JSON rollups under `data/`
- preserved `assets/` content when regenerating over an existing Pages checkout
- preserved evidence-bearing per-scenario `data.json` fields when an existing
  record uses the same schema
- copied repo screenshots from `docs/images/` and `docs/testing/scenarios/` into
  the existing Pages asset path conventions

Each scenario record also includes structured `product_context`, `flow_steps`,
`execution`, `review_grounding`, `launch_assessment`, `quality_review`,
`coherence`, `readiness`, `evidence.missing`, `evidence.placeholders`,
`evidence_provenance`, `before_after_deltas`,
`revalidation_status`, `owner_status`, `instrumentation_gaps`, and `risks`
fields. These are intentionally first-class JSON fields so future importers can
populate observed runs without scraping prose.
This taxonomy also includes explicit data integrity/state sync, navigation
state/restoration, device/viewport coverage, and media session/background audio
continuity dimensions.

The focused next-wave manifest lives in
`docs/testing/report-generator/data/next-wave-foundation-onboarding.json`. It is
planning metadata only: it defines screenshot requirements, metrics to collect,
UI/UX/Liquid Glass checks, and issue-filing gates for the next Foundation
onboarding evidence wave, but it does not change execution status or verdicts.

## Review Skill Grounding

This pass used:

```sh
npx skills search "UI UX mobile Liquid Glass design review accessibility"
npx skills use casper-studios/casper-marketplace@liquid-glass
npx skills use charleswiltgen/axiom --skill axiom-design
npx skills use charleswiltgen/axiom --skill axiom-accessibility
npx skills use charleswiltgen/axiom --skill axiom-performance
npx skills use phazurlabs/ux-ui-mastery --skill "mobile ux design"
```

`data/skill-grounding.json` records these selected skills and the template
impact:

- Liquid Glass: content-first hierarchy, glass limited to navigation/control
  chrome, adaptive tinting, semantic emphasis, and Reduce
  Motion/Transparency expectations.
- Axiom design/accessibility/performance: HIG, semantic colors, SF typography,
  VoiceOver, Dynamic Type, contrast, touch targets, gesture alternatives,
  launch/tap/screen-settle latency, scroll hitches, memory, CPU, and measured
  performance discipline.
- Mobile UX design: one primary action, thumb-zone reachability,
  interruption/resume behavior, visible gesture alternatives, platform
  navigation conventions, and latency-as-UX budgets.

UI, UX, accessibility, performance, and product-coherence scores must stay at
`2` or below until the relevant evidence and grounded critique are attached.
