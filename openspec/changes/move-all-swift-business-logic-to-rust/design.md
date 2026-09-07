## Context

See [proposal.md](proposal.md) for motivation. Pod0 already has a Rust core, UniFFI and Kotlin bindings, native capability hosts, architecture inventories, and a temporary Swift-policy exception ratchet. The remaining problem is structural: ownership is inferred partly from file classification, while active product policy also exists in files classified as shared or native. Swift therefore still owns durable facts, selects product outcomes, and sometimes dispatches effects without a Rust-authored leased request.

This change spans agent, voice, transcript, workflow, settings, usage, playback, library, search, import/export, and publication paths. It must preserve Apple-framework execution and secret custody in the native process, avoid dual writers during migration, tolerate crashes and ambiguous effects, and keep every changed source file below the repository's 500-line hard limit.

The delta specs define the required end state. This design defines the transition protocol and ownership seams used to reach it.

## Goals / Non-Goals

**Goals:**

- Establish one Rust owner for every product fact and decision while retaining native ownership of literal platform execution.
- Make every cross-boundary mutation replayable, fenced, correlated, and observable.
- Migrate in independently verifiable domain waves without a period of dual authority.
- Replace the file-label allowlist with semantic enforcement and a zero-exception gate.
- Delete dormant and superseded Swift policy instead of reproducing it in Rust.
- Keep domain behavior in focused modules rather than constructing one generic state machine or CRUD layer.

**Non-Goals:**

- Reimplementing AVFoundation, MediaPlayer, Speech, Keychain, OAuth browser flows, URLSession transport, BGTask scheduling opportunities, notifications, widgets, Spotlight presentation, share sheets, or file pickers in Rust.
- Standardizing unrelated domain models behind a universal repository, command, or workflow abstraction.
- Preserving obsolete Swift APIs, compatibility aliases, dual-write fallbacks, or rollback paths that restore Swift as a business authority.
- Redesigning product UI, typography, navigation, or localization.

## Decisions

### 1. Use a common transition protocol with domain-specific state machines

Each domain exposes typed Rust commands and queries through the existing facade/binding layers. A mutation is handled as one durable transition that validates its preconditions, records authoritative state and facts, and enqueues any internal commands or external effect intents atomically. Results include typed dispositions such as applied, rejected, duplicate, stale, or deferred.

The protocol is shared; command types, state, invariants, and reducers remain domain-specific. This preserves clear ownership and prevents a generic workflow engine from becoming a second policy language.

**Alternatives considered:**

- A single generic state machine or CRUD repository would reduce surface area initially, but would erase domain invariants and encourage untyped policy outside Rust.
- Direct synchronous Rust-to-native calls would be simpler for happy paths, but cannot make commit/effect ambiguity, cancellation, or replay safe.

### 2. Cross-domain consequences use durable internal commands

When one domain decision must mutate another domain, the source transition writes a typed internal command to a durable outbox. The owning target domain consumes it idempotently and records its own transition. In-process convenience calls may wake consumption but never bypass the outbox.

This includes product-mutating agent tools such as playback, rate, download, library, or workflow changes. The permission matrix describes authorization and native capability access; it cannot designate Swift as the mutation owner. The implementation will reconcile the matrix so product mutations enter the owning Rust domain, while truly native effects still use leased capability requests.

**Alternatives considered:**

- Direct domain-to-domain mutation creates hidden coupling and cannot be replayed independently.
- Treating all agent tools as native capabilities conflates authorization with ownership and preserves the current bypass.

### 3. Native effects use exact leased requests and raw correlated observations

Rust persists an effect intent before native execution and issues an immutable request containing an effect id, attempt or fence token, capability kind, exact parameters, and cancellation correlation. Native code performs only that literal request. It does not add defaults, select alternatives, retry, back off, or reinterpret errors.

Native completion returns a bounded observation: identifiers, status codes, provider payload metadata, timed words, bytes, file handles or staged URLs, and platform error data as appropriate. Rust maps that observation to a semantic outcome and advances authoritative state. Repeated delivery is idempotent; stale fences are rejected; ambiguous results remain reconcilable rather than being declared failed.

For long-running effects, cancellation is another correlated request. Cancellation requested, cancellation observed, effect completion, and effect committed remain distinct states.

**Alternatives considered:**

- A broad provider-neutral native service would keep orchestration convenient but would continue to own fallback and outcome policy.
- Passing only success/failure booleans would lose evidence needed for deterministic interpretation and reconciliation.

### 4. Credentials stay native; authorization and provider policy move to Rust

Keychain values, OAuth tokens, and other secrets remain opaque native handles. Rust decides whether a provider action is authorized, which configured provider/model/voice is selected, and what exact operation is requested. Native code resolves the referenced credential only while executing the lease and returns non-secret observations.

Settings screens may collect and display native-safe values, but Rust owns validation state, category policy, model/voice availability, preview eligibility, and persisted product settings. Keychain and iCloud are capability transports, not competing settings stores.

**Alternatives considered:**

- Moving secret bytes into Rust would enlarge the exposure surface without improving product ownership.
- Keeping validation and provider selection in Swift would preserve behavior forks between clients.

### 5. Transcript semantics are Rust-owned from raw provider evidence

Transcript providers and Apple Speech remain native executors where platform APIs require it. They return raw response envelopes, byte streams, audio observations, and provider timing/speaker fields within explicit bounds. Rust owns parsing, normalization, stable segment and speaker identity, provider-phase state, retryability, and semantic failure classification.

The transcript host cannot infer provider phase from the request type or fabricate semantic failures. It reports what was attempted and observed; the Rust workflow decides what that means.

**Alternatives considered:**

- Sharing provider parsers between native clients appears efficient but makes the native layer a product authority and produces cross-platform drift.
- Returning pre-normalized transcript objects hides lossy transformations and weakens replay evidence.

### 6. Durable Swift stores migrate once into Rust-owned schemas

Settings, category configuration, usage/cost history, workflow schedules, and any remaining library or playback facts receive explicit versioned Rust schemas and imports. Migration first snapshots the legacy source, validates the import, records an authority marker, and then disables the Swift writer. A migration is idempotent and can resume after interruption.

The cost and usage schema preserves the current retention and bounds as explicit Rust policy until separately changed. Native caches and staged files may remain for platform execution, but they are rebuildable and cannot be read as authoritative product state.

**Alternatives considered:**

- Long-lived dual writes would complicate conflict resolution and make ownership unverifiable.
- Reading legacy stores indefinitely would leave a hidden second schema and prevent deletion.

### 7. Rust authors projections, queries, and export/publication plans

Library ordering, episode-derived metadata, search ranking, continue-listening windows, playback eligibility and queue policy, imports, scheduling, and workflow reconciliation become Rust queries or transitions. Swift renders returned projections and executes platform media primitives.

For export, sharing, Spotlight indexing, generated clips, and publication, Rust produces an immutable plan containing selected records, ordering, names, metadata, redaction decisions, limits, and exact output or transport intent. Native code renders or transmits the plan and returns raw receipts. Formatting that is purely tied to a platform API may stay native only when it cannot alter product selection or meaning.

**Alternatives considered:**

- Leaving selection in share sheets or exporters is convenient but makes output meaning dependent on the client.
- Moving all binary encoding into Rust would add platform duplication without improving decision ownership.



**Alternatives considered:**

- Leaving semantic event construction in Swift would violate the same ownership boundary as other publication paths.

### 9. Enforcement is semantic and ratchets to zero

The ownership inventory assigns every product source, durable input, mutable fact, request, effect, and observation to exactly one owner. Architecture checks scan all production Swift and Kotlin sources for prohibited decision patterns and direct effect dispatch, regardless of directory or current classification. The existing exact-symbol checks remain useful but are subordinate to the complete semantic rules.

Each prohibited category has a negative fixture that must fail the checker. Binding generation and parity checks ensure Swift and Kotlin expose the same Rust authority. The final gate rejects any native business-policy exception; temporary exemptions must be removed in the same wave that eliminates their code.

**Alternatives considered:**

- Extending the eleven-file allowlist would keep missing policy in files classified as native or shared.
- Review-only enforcement would regress as the codebase evolves.

### 10. Delete unowned and superseded paths at each cutover

Dormant owner-question handling, review prompting, unused feed parsing and Spotlight reindexing, legacy workflow stores, migration-only repositories, obsolete adapters, aliases, and compatibility branches are deleted once references and migration preconditions are proven absent. A moved behavior does not leave its old API as a forwarding shim unless a platform framework requires that exact adapter.

Source files are split along ownership seams before reaching 300 lines where practical and must remain below 500 lines. Generated binding files are treated according to their generator, not manually expanded.

**Alternatives considered:**

- Keeping dormant code for possible reuse preserves misleading owners and expands the enforcement surface.
- Large coordinator files make it difficult to prove which layer owns a decision.

## Risks / Trade-offs

- [Umbrella scope causes an unreviewable cutover] → Deliver ownership-complete domain waves with their own migrations, tests, and deletion gates; do not mix partial authority from several domains in one wave.
- [A crash occurs after a durable transition but before or during an effect] → Persist intents atomically, use idempotency keys and fences, and reconcile ambiguous observations before retrying.
- [Shadow comparison accidentally becomes dual authority] → Permit read-only comparison telemetry only; Rust is the sole writer as soon as a domain authority marker is committed.
- [Provider payloads or files exceed the FFI boundary] → Use bounded envelopes and opaque staged handles, with Rust-owned limits and integrity metadata.
- [Native semantic behavior hides behind generic helpers] → Inventory effects and observations, add semantic negative fixtures, and review all direct framework/provider dispatch sites.
- [Migration loses user state] → Snapshot, import idempotently, compare counts and domain invariants, and retain a recoverable pre-cutover backup until the wave's verification window closes.
- [Performance regresses through chatty FFI calls] → Expose bounded batch queries and projections, benchmark hot paths, and keep thresholds in the final verification matrix.
- [Generated bindings drift] → Regenerate both Swift and Kotlin bindings from the same Rust interface and require parity in CI.
- [Native UI work expands the change] → Preserve existing UI and system-font rules; change presentation only where an obsolete policy API must be removed.

## Migration Plan

1. Reconcile overlapping changes and freeze a complete ownership/effect inventory. Record baseline architecture checks, tests, storage versions, and performance measurements.
2. Land the transition primitives, internal-command outbox, leased native-effect contract, raw observation contract, bindings, semantic checker, and negative fixtures without changing domain authority.
3. Migrate settings, categories, provider configuration state, usage, and cost history. Import once, commit the Rust authority marker, disable Swift writers, verify, and delete legacy stores.
4. Migrate transcript parsing/normalization and provider-phase truth. Then cut agent, voice, model selection, approval, provider authorization, and product-mutating tools to Rust commands and leased effects.
5. Migrate workflow configuration, schedules, reconciliation, background-work interpretation, library/search projections, imports, playback policy, queue policy, and handoff decisions. Keep framework execution native.
7. Delete dormant and superseded code, remove all temporary exceptions and compatibility paths, regenerate both binding targets, and run the complete crash/replay/concurrency, domain-scenario, architecture, and performance gates required for #219.

Each wave uses the same cutover sequence: land dormant Rust support, migrate and validate data, stop the Swift writer, commit the authority marker, enable Rust reads/writes, exercise crash and replay cases, then delete the legacy path and its exemption.

Rollback is forward-only after an authority marker is committed. Before that point, the dormant Rust path can be disabled. After that point, recovery restores the pre-cutover snapshot into a fresh Rust schema or applies a corrective Rust migration; it never re-enables Swift writes or dual authority. External effects already observed are reconciled by idempotency key and receipt rather than blindly replayed.
