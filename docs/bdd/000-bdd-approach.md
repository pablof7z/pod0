# BDD in Pod0: readable contracts over the supported facade

- **Date:** 2026-07-29
- **Status:** CURRENT PRACTICE. Every scenario in `features/` executes on
  every `cargo test` run; pod0 has deliberately not adopted a backlog of
  aspirational scenarios.
- **Lineage:** modelled on NMP's `docs/bdd/000-bdd-approach.md`, adapted to
  pod0's constraints (synchronous facade, rusqlite store, 300/500 file
  limits, cargo-deny license gates).

## 1. What the scenarios drive

Scenarios drive `Pod0Facade` — the single app-owned native/core boundary
(`rust/FACADE_CONTRACT.md`), never `pod0-application` underneath it. The
facade is what Swift and Kotlin actually call, so a scenario that passes here
is a claim about the surface the platforms see, and it survives any internal
rewrite that keeps the contract. The seven contract operations map cleanly
onto Gherkin: `dispatch` is a `When`, `snapshot` and subscriber deliveries
are what `Then` reads, and `next_host_requests` / `record_host_observation`
let the suite play the native host's role with fixture bytes.

Each scenario gets a fresh world: a temp directory, an authoritative store
prepared through the same public bootstrap exports the production Swift shell
uses (`SharedLibraryBootstrap.swift`) with empty legacy sources, and a real
`Pod0Facade::open` over it. `Given` steps stage plain data and do no I/O;
the first acting step starts the world lazily. Relaunch scenarios reopen the
same store the way process restart does — durability claims are observed
through the facade, never by inspecting the database.

There are no bounded-wait budgets here: the facade is a synchronous
command/state surface, so a scenario that needs to poll or sleep is evidence
of a design problem, not a missing helper.

## 2. Suite structure

```text
features/                  # repo root, organised by domain
  library/                 # what the app-visible library shows
  operations/              # command lifecycle: cancel, retry, typed failure
rust/crates/pod0-bdd/
  src/                     # cucumber-free fixture builders (unit-tested)
  tests/bdd/main.rs        # harness = false entry point + tag filtering
  tests/bdd/world/         # {store,staging,actions,observe}.rs
  tests/bdd/steps/         # given.rs, when.rs, then/{library,operations,host_work}.rs
```

All cucumber-typed code lives inside the `bdd` test target so `cucumber`
stays a dev-dependency: cargo-deny prunes dev-dependencies from its license
graph, which keeps the BlueOak-1.0.0 `synthez` proc-macro subtree (pulled by
cucumber's derive/attribute macros) out of the checked production graph
without weakening `rust/deny.toml`'s allow-list. No production crate depends
on `pod0-bdd`, and nothing from it ships.

`Then` steps may read exactly four observables, all on the public facade
surface: projection snapshots, deliveries to a live subscriber, drained host
work, and typed observation receipts. Every negative claim ("does not
list", "no further deliveries") first proves something existed to observe,
via the `nothing_to_observe!` macro in `steps/then/mod.rs` — a check that
cannot fail is not coverage.

## 3. Tags

- `@wip` — built and BROKEN, with a filed report. Always excluded by the
  runner so a known gap never masquerades as a passing proof.
- `@designed` — NOT BUILT YET; the scenario is the agreed acceptance
  criterion for building it, carries no step definitions, and must never
  reach the runner. Removing the tag is the definition of done. Pod0
  currently ships zero of these; adding one is a deliberate product
  decision, not a default.

The runner excludes both in `tests/bdd/main.rs` and fails on any skipped
(undefined) step, so the step catalog is closed from both sides: scenarios
compose from the reviewed vocabulary in `steps/`, and a new step lands only
together with the first scenario that needs it.

## 4. Style rules

- One promise per scenario title; stage the world, act once, assert only
  through the four observables.
- Prose names people-visible things (podcasts, episodes, the library, the
  host) — never Rust types, store tables, or module names.
- Command identities and observation timestamps derive from one monotonic
  fixture counter; there is no injected clock and no wall-clock read in any
  step.
- Every file in `rust/crates/pod0-bdd` obeys the repository-wide 300-line
  soft / 500-line hard limit, enforced by the existing
  `scripts/check_file_lengths.py` gate (it already scans `rust/crates`);
  `.feature` files are prose and are not scanned.
