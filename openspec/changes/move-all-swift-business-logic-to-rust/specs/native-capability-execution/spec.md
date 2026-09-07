## Purpose

Defines how platform-specific code executes exact Rust-authorized capabilities without becoming a second product-policy, persistence, retry, or semantic-outcome owner.

## ADDED Requirements

### Requirement: External effects require durable authorization
Every provider, network, file, media, notification, signing, publication, speech-generation, or other externally observable capability SHALL be represented by a persisted Rust effect intent before native execution begins.

#### Scenario: Capability is requested without a lease
- **WHEN** native code receives a request without a valid persisted lease and attempt identity
- **THEN** it refuses execution and reports no success observation

#### Scenario: User directly initiates an effect
- **WHEN** a user taps an action that requires an external effect
- **THEN** the intent first enters the Rust owner and native execution starts only after the corresponding effect is claimable

### Requirement: Native execution is literal and exact
A native capability SHALL execute the exact typed request claimed from Rust. It SHALL NOT select a provider or model, change arguments, add product defaults, widen scope, choose fallback, initiate retry, or start related product work.

#### Scenario: Optional argument is absent
- **WHEN** an exact request omits an argument such as a voice, route, provider option, or target
- **THEN** native code reports an invalid or unsupported request instead of supplying its own product default

#### Scenario: Native provider fails
- **WHEN** a provider capability fails
- **THEN** native code reports the bounded raw failure evidence once and does not retry or select another provider

### Requirement: Native observations preserve raw evidence
Native capabilities SHALL return correlated, bounded observations containing the raw facts Rust needs, including status codes, provider operation identity, response metadata, byte counts, timestamps, cancellation evidence, and platform error identity where safe. Native code SHALL NOT collapse raw evidence into a durable product outcome.

#### Scenario: HTTP request returns an error response
- **WHEN** a provider returns a non-success HTTP status and headers
- **THEN** native code reports the status and bounded relevant headers so Rust determines authentication, rate-limit, retry, rejection, or terminal meaning

#### Scenario: OS capability is unavailable
- **WHEN** an operating-system capability cannot execute the request
- **THEN** native code reports the platform observation without changing product state

### Requirement: Effect attempts are fenced and idempotent
Every native effect attempt SHALL carry request, lease, attempt, cancellation, and fence identity sufficient to reject stale or duplicate observations. Native recovery SHALL reattach only to the exact recoverable platform operation and SHALL NOT repeat an externally ambiguous operation.

#### Scenario: Late callback arrives after cancellation
- **WHEN** a provider or operating-system callback arrives for a cancelled or superseded attempt
- **THEN** Rust rejects it by identity and no product mutation or success fact is committed

#### Scenario: Process dies after possible remote success
- **WHEN** the process terminates after an external request may have succeeded but before its observation commits
- **THEN** recovery reattaches using durable external identity or records outcome unknown without blindly repeating the effect

### Requirement: Cancellation reaches the executing capability
Rust-owned cancellation SHALL produce a correlated native cancellation request when a capability can be cancelled. Native code SHALL stop local work, preserve required recovery evidence, and report cancellation without deciding the durable workflow outcome.

#### Scenario: Download is cancelled
- **WHEN** Rust cancels an active native download attempt
- **THEN** native code cancels the exact platform task, preserves bounded resume evidence if available, and reports the correlated cancellation observation

### Requirement: Credentials remain native and authorization remains Rust-owned
Secrets SHALL remain in Keychain or equivalent platform security facilities. Native code MAY resolve a secret only for an exact authorized request and SHALL return bounded availability or execution evidence without exposing the secret to Rust, logs, projections, or activity facts.

#### Scenario: Credential is missing
- **WHEN** an authorized capability cannot resolve the requested credential
- **THEN** native code reports credential-unavailable evidence and neither side records or logs secret material

### Requirement: Platform presentation and primitives remain native
Native shells SHALL retain layout, localization, accessibility, navigation, haptics, AVFoundation or Media3 execution, audio capture, Apple or Android speech frameworks, Keychain prompts, OAuth UI, BGTask entry points, notifications, widgets, Spotlight, share sheets, and literal file/media operations. These surfaces SHALL consume Rust projections or typed capability requests and SHALL NOT own product policy.

#### Scenario: Share sheet is presented
- **WHEN** Rust authorizes a prepared share artifact
- **THEN** native code writes or renders the exact artifact and presents the platform share sheet without changing selection, redaction, provenance, or retention policy

#### Scenario: Playback media command executes
- **WHEN** Rust authorizes an exact playback platform transition
- **THEN** native code performs the media primitive and returns playback observations while Rust remains the playback state owner

### Requirement: Native buffers and files are non-authoritative
Transient streaming buffers, staged files, resume data, widget snapshots, and platform handles SHALL NOT be treated as committed product truth. Rust SHALL explicitly adopt or reject staged artifacts before they appear in authoritative projections.

#### Scenario: Generated audio is staged
- **WHEN** native generation finishes and writes a staged audio file
- **THEN** the file remains uncommitted until Rust validates and adopts the correlated artifact observation
