# ADR-0008: Agent actions, permissions, commits, and NMP publication

- Status: Accepted
- Date: 2026-07-22
- Decision owners: Pod0 application architecture
- Related issues: #60, #131, #132, #133, #134, #135, #136, #137, #138

## Context

The interactive agent currently receives model-authored JSON, parses it into
`[String: Any]`, and lets Swift dispatch 46 tools. Those tools span private
reads, durable writes, playback, paid provider requests, destructive deletion,
artifact generation, and public upload. Prompts and skill activation provide
advisory gating, but no single durable authority, proposal, commit, or recovery
contract exists. Swift also composes Nostr signatures independently of the
Pod0-owned `pod0-nmp` runtime.

This is cross-platform, security-sensitive product policy. Native code must
still present conversations and approvals and execute platform primitives, but
model output, an enabled skill, or a native callback cannot authorize or commit
durable action.

## Decision

Pod0 Rust is the sole owner of interactive turn state, tool schemas and typed
actions, validation, authorization, capability admission, idempotent commit,
audit, cancellation, recovery, and bounded projections. Native shells render
those projections and execute only typed capabilities. Generic NMP owns Nostr
identity, signing, routing, receipt, and relay facts behind `pod0-nmp`.

The machine-readable [permission matrix](../agent-tool-permissions.json)
classifies every current tool exactly once. CI compares it to canonical Swift
tool declarations, rejects unclassified additions, and rejects privileged
actions without durable authority.

## Authority levels

- `none`: a bounded local read or transient interaction with no privileged
  fact. It never implies authority for a later tool.
- `durable_turn_grant`: Rust records an exact user turn, bounded action family,
  targets, proposal revision, and expiry before execution.
- `durable_scoped_grant`: a persisted provider/data scope and budget combines
  with a bounded turn grant. Possessing a credential is not a grant.
- `one_shot_approval`: the user approves one exact proposal digest and revision;
  argument, target, cost, privacy, or revision changes invalidate it.

External effects, private/secret access, destructive changes, future scheduled
effects, paid generation, and publication fail closed unless the matrix's
durable authority is present. A model request, prompt text, `use_skill`, native
UI state, prior approval, or provider acceptance is never authority. Native
approval presentation reports raw approve, deny, or dismiss observations; Rust
validates whether that observation authorizes the current proposal.

## Typed application contract

Stable identifiers include `ConversationId`, `TurnId`, `ProposalId`,
`AuthorizationId`, `CapabilityRequestId`, `CommitId`, `ArtifactId`, and
`PublicationId`. Every command includes expected revisions and is idempotent.
Every durable timestamp is integer milliseconds supplied to Rust or its
injected clock.

Commands are fire-and-forget state transitions:

- `StartAgentTurn`, `SubmitAgentInput`, and `CancelAgentTurn`;
- `ApproveAgentProposal` and `DenyAgentProposal` with proposal digest/revision;
- raw provider, approval, capability, signer, and publication observations;
- explicit recovery/reconciliation commands, never blind effect retries.

Bounded projections expose a selected conversation page, current turn stage,
validated proposal summary, requested authority, active capability request,
commit/artifact reference, and exact safe failure or ambiguity state. They do
not expose databases, full journals, secrets, arbitrary JSON, NMP engine types,
relay URLs, or unbounded transcript/chat history.

Rust host requests are closed typed variants such as `ExecuteModelTurn`,
`PresentAgentApproval`, `ExecutePlaybackPrimitive`, `ExecuteProviderRequest`,
`ReadSecureCredential`, and `StageArtifactBytes`. Native returns correlated raw
observations. It does not classify errors, select retries, expand permission,
mutate authoritative domain storage, or declare success.

## Proposal, effect, and commit lifecycle

The durable lifecycle is:

1. Rust records the user input and creates a fenced provider request.
2. Native returns bounded model output; Rust parses a closed typed action.
3. Rust validates identifiers, arguments, privacy, cost, current revisions,
   tool availability, and the matrix authority.
4. Rust either rejects, requests exact approval, or records authorization.
5. Rust persists an effect fence before emitting a host request.
6. Native reports one correlated raw result, cancellation, or ambiguity.
7. Rust validates the active fence and atomically commits the action, audit
   fact, selected artifact, and conversation transition.

The explicit states are `proposed`, `invalid`, `approval_required`,
`authorized`, `executing`, `observed`, `commit_pending`, `committed`, `denied`,
`cancelled`, `blocked`, and `outcome_ambiguous`. Late or duplicate observations
cannot advance a stale fence. Process loss reconstructs the exact state. An
ambiguous external effect stays ambiguous until evidence resolves it; it never
becomes retry permission.

## Nostr and publication boundary

Pod0 Rust defines Pod0 event nouns, validates semantic content, selects public
versus private intent, and creates one `PublicationId`. `pod0-nmp` is the only
direct NMP engine owner and converts an authorized Pod0 publication into a
generic `WriteIntent`. The application facade accepts no relay URL or generic
NMP routing primitive.

Public byte upload, file access, biometrics, and secure credential access remain
typed native capabilities. Nostr event composition, author binding, signer
selection, routing, delivery, and receipt facts do not occur in Swift. Private
recipient delivery uses NMP's inbox or `PrivateNarrow` semantics and fails
closed; it never falls back to public author outbox routing.

For every publication, Rust persists a stable correlation token derived from
the immutable `PublicationId` and semantic revision before calling
`publish_tracked`. The same token may only ever name the same semantic write.
Rust persists the returned `ReceiptId` immediately as a secondary direct
reattachment key. After restart it reattaches by receipt id when present and by
correlation token otherwise.

The pinned NMP revision is
`68310f88a31bf80e6b73d018b1374e73efda0041`. At this revision, a caller that
omits `WriteIntent.correlation` still has a crash gap after durable NMP
acceptance but before Pod0 persists the returned receipt id. Pod0 closes that
gap structurally by requiring the pre-persisted correlation token and using
`reattach_by_correlation`; a dependency regression that removes either
capability blocks publication.

Receipt `Accepted`, `Cancelled`, `AwaitingCapability`, `Signed`, `Routed`,
`AwaitingRelay`, `AwaitingAuth`, `RetryEligible`, `HandoffAmbiguous`, `Sent`,
`Acked`, `Rejected`, `GaveUp`, `PersistenceBlocked`,
`RoutePersistenceBlocked`, `OutcomeUnknown`, `ReplaceableConflict`, and
`Failed` facts remain distinct. Stream closure is not success. An ACK is not an
artifact commit. `OutcomeUnknown` is not retry permission. Explicit
cancellation is attempted only before signature; typed cancellation refusal is
preserved rather than reclassified.

Pod0 may store a bounded receipt fold and audit link, but it does not create a
second event cache or rewrite NMP facts. NMP receipt state cannot authorize a
tool, complete an agent turn, or prove a generated artifact is valid.

## Migration and rollback

Issues #133–#137 establish the Rust turn/tool reducer, native capabilities,
durable conversation/artifact ownership, NMP publication, and secure signer
path. Issue #138 performs inspect, versioned backup, staged import, validation,
authority-marker commit, and immediate deletion of replaced Swift writers.

Before the marker commits, rollback discards staged Rust state and leaves the
Swift writer active. After commit, Swift cannot resume authority; rollback uses
the tested immutable export path. No dual write is permitted. Native
conversation rendering, transient token animation, provider networking,
AVFoundation, approval sheets, file primitives, and platform credential UI
remain.

The cutover deletes or reduces `AgentTools` dispatch/schema policy,
`AgentChatSession` turn/tool decisions, Swift chat/run-log durability,
`AgentNostrSigner`, direct Blossom/publication orchestration, and store-mutating
tool adapters. Every surviving adapter must correspond to a typed Rust host
request and raw observation.

## Required falsifiers

- every Swift tool maps to exactly one matrix row and privileged tools cannot
  be downgraded to no authority;
- proposal digest/revision, replayed approval, stale result, duplicate result,
  cancellation, and process-death tests;
- denied or absent grants produce no native request and no durable mutation;
- external ambiguity does not retry or commit;
- public/private routing, missing signer, signer rejection, ACK, rejection,
  persistence blockage, `OutcomeUnknown`, not-found, and correlation
  reattachment tests;
- generated Swift/Kotlin parity, Android-compatible builds, bounded FFI,
  deterministic shutdown, no polling, and deletion of replaced Swift owners.

## Consequences

- iOS keeps native interaction quality and provider/platform integrations.
- Android later consumes identical durable agent decisions and permission
  semantics instead of reproducing Swift behavior.
- Tool additions require an explicit class, authority, typed action, executor,
  tests, and deletion/capability disposition.
- NMP remains generic; Pod0 nouns and commits remain in Pod0 crates.

## Rejected alternatives

- **Prompt-only confirmation:** model text is not durable authorization.
- **Native approval as policy:** creates platform-specific permission owners.
- **Arbitrary JSON RPC:** permits schema drift and duplicate validation.
- **Swift Nostr signing/routing:** bypasses the one generic fact owner.
- **Receipt ACK as agent completion:** conflates delivery with product commit.
- **Dual-write cutover:** cannot prove ordering or recover one source of truth.
