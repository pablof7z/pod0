## Purpose

Defines a single durable conversation authority for text and voice so voice turns cannot bypass Rust-owned state, cancellation, recovery, or approval policy.

## ADDED Requirements

### Requirement: Shared durable conversation
Voice and text interactions SHALL use the same durable conversation identity, message history, turn revision, and terminal state, and SHALL resume that same state after process restart.

#### Scenario: Voice continues a text conversation
- **WHEN** a listener starts a voice turn in a conversation previously used by text chat
- **THEN** the voice turn observes the same history and commits its result to the same conversation

#### Scenario: The app relaunches during a voice conversation
- **WHEN** the process terminates and the conversation is reopened
- **THEN** the persisted conversation and exact turn outcome are recovered without duplicating a model call or committed side effect

### Requirement: Cross-modal turn exclusivity
At most one active turn SHALL exist in a conversation regardless of whether it was initiated by text or voice.

#### Scenario: Text sends while voice is active
- **WHEN** a voice turn is active and text attempts to send another turn in the same conversation
- **THEN** the text send is rejected or disabled using the shared authoritative availability state

#### Scenario: Voice sends while text is active
- **WHEN** a text turn is active and voice attempts to begin another turn in the same conversation
- **THEN** the voice send is rejected or delayed without creating a second authority

### Requirement: Rust-acknowledged voice cancellation
Barge-in and explicit stop SHALL request cancellation against the expected authoritative turn revision. No local-only cancellation SHALL be presented as termination of the underlying turn.

#### Scenario: Cancellation precedes provider dispatch
- **WHEN** barge-in occurs before provider work is dispatched
- **THEN** the turn becomes durably cancelled and no provider work begins

#### Scenario: Cancellation occurs during streaming
- **WHEN** barge-in occurs while provider output is streaming
- **THEN** local speech stops promptly, Rust acknowledges cancellation within the defined latency budget, and late provider output cannot revive the turn

#### Scenario: Cancellation follows a committed side effect
- **WHEN** barge-in occurs after an approved side effect has committed
- **THEN** the committed result remains durable, no second side effect occurs, and the conversation records the truthful post-commit terminal outcome

### Requirement: Explicit terminal failures
Blocked, ambiguous, provider-failed, and cancelled states SHALL terminate the voice turn with distinct user-observable outcomes and SHALL not be converted into a fabricated assistant response.

#### Scenario: Provider outcome is ambiguous
- **WHEN** the core cannot prove whether submitted provider work completed
- **THEN** voice ends with an explicit ambiguous outcome and does not automatically retry the unsafe work

### Requirement: Explicit voice approvals
An approval-required voice turn SHALL present the proposed action in speech and visible UI and SHALL require an explicit user confirmation before the action executes. Ambient or hands-free mode SHALL never silently approve.

#### Scenario: User denies a proposed action
- **WHEN** the listener declines the spoken approval prompt
- **THEN** the proposal is durably denied and no protected side effect executes

#### Scenario: Approval presenter is unavailable
- **WHEN** a turn requires approval but voice cannot present and capture explicit confirmation
- **THEN** the turn fails closed and the action remains unexecuted

### Requirement: No production fallback authority
Production composition SHALL contain no stub voice-turn delegate, local conversation store, Swift-side tool dispatcher, or approval bypass.

#### Scenario: Production dependency is missing
- **WHEN** the shared conversation adapter cannot be composed
- **THEN** Voice Mode remains unavailable rather than falling back to a local or stub implementation
