## Outcome

<!-- What user or engineering outcome becomes true? -->

## Verification

<!-- Commands, test cases, simulator/device evidence, migration fixtures. -->

## Cross-platform ownership review

Complete this section for product, state, workflow, agent, persistence, or
platform-boundary changes. For documentation-only or purely visual work, mark
it not applicable and explain why.

- [ ] **1. Classification:** Is this presentation behavior, a platform
  capability, or durable product logic?
- [ ] **2. Durable format:** Does this introduce a new durable model, identifier,
  schema, file, database row, event, or state format?
- [ ] **3. Android need:** Could Android need the same behavior or persisted
  fact?
- [ ] **4. Long-term owner:** Where is the long-term source of truth?
- [ ] **5. Ownership choice:** Is the logic **Shared Rust now**, **Native by
  design**, **Temporary Swift behind a migration-safe boundary**, or a
  **Planning/research/decision record**?
- [ ] **6. Temporary deletion:** If temporary, what linked issue removes or
  migrates it? Temporary Swift without an issue is not mergeable.
- [ ] **7. Typed boundary:** Is the boundary typed and testable through commands,
  projections, domain events, host requests, or raw host observations?
- [ ] **8. Platform leakage:** Does a supposedly shared API assume UIKit,
  AVFoundation, URLSession, Swift `Date`, Apple paths, or another Apple-only
  concept?
- [ ] **9. Termination safety:** Could process termination, duplicate callbacks,
  or stale revisions create inconsistent durable state or repeat a paid effect?
- [ ] **10. Deletion:** What obsolete code, schema, exception, feature flag, or
  duplicate owner is deleted when this work completes?

### Ownership declaration

- Classification:
- Current source of truth:
- Source of truth after this PR:
- Typed interface or native capability boundary:
- Migration/deletion issue, if temporary:
- Obsolete code deleted by this PR:

Architecture source: [`docs/architecture/README.md`](../docs/architecture/README.md).

<details>
<summary>Classification examples</summary>

- `PlaybackState` queue/resume/completion policy migrates to shared Rust;
  `AudioEngine` and AVFoundation execution remain native capabilities.
- Normalized transcript, chapter, and evidence semantics belong in Rust;
  Apple Speech remains a native capability and provider request/recovery policy
  follows the shared workflow owner.
- SwiftUI rendering/navigation is native; `AppStateStore` is a temporary Swift
  durable owner. Views dispatch domain methods/intents rather than mutating
  `AppState` or opening repositories directly.

</details>

## User-facing change

- [ ] This changes what iPhone users see or can do, and
  `App/Resources/whats-new.json` has a unique current UTC entry.
- [ ] This is internal-only; no whats-new entry is required.

## Safety checklist

- [ ] No serif typeface was introduced.
- [ ] Touched files remain below the 500-line hard limit; approaching 300 lines
  prompted a split review.
- [ ] User data has a tested migration/rollback path when applicable.
- [ ] Replaced ownership is deleted in the same vertical slice; no durable dual
  writer remains.
