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

## Review Skill Grounding

This pass used:

```sh
npx skills search "Liquid Glass iOS mobile UI UX polish accessibility frontend design"
npx skills use heyman333/atelier-ui@ios-glass-ui-designer --dangerously-accept-openclaw-risks
npx skills use phazurlabs/ux-ui-mastery --skill "Mobile UX Design" --dangerously-accept-openclaw-risks
```

`data/skill-grounding.json` records these selected skills and the template
impact:

- iOS Glass UI Designer: iOS-native hierarchy, restrained glass/material use,
  system typography, semantic foreground styles, safe areas, native sheets and
  navigation, and Reduce Transparency/Motion expectations.
- Mobile UX Design: user-goal framing, thumb-zone reachability, 44 pt/48 dp
  touch targets, interruption/resume behavior, platform navigation conventions,
  and performance-as-UX budgets.
- The Mobile UX Liquid Glass reference: contrast, Increase Contrast, Reduce
  Transparency, Dynamic Type, and oldest-device performance checks.

UI, UX, accessibility, performance, and product-coherence scores must stay at
`2` or below until the relevant evidence and grounded critique are attached.
