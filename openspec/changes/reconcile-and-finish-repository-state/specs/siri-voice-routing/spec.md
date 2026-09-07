## Purpose

Defines the final gated Siri and Shortcuts voice entry point using the same qualified conversation and native audio paths as in-app Voice Mode.

## ADDED Requirements

### Requirement: Voice routing remains gated on prerequisites
Siri and Shortcuts voice-agent routing SHALL remain unavailable until repository delivery checks, shared voice authority, approval behavior, and the complete physical-device audio matrix all pass for the same candidate.

#### Scenario: A prerequisite is missing
- **WHEN** any required voice, approval, audio, simulator, device, or hosted validation is incomplete
- **THEN** voice-agent App Intents are not advertised or routed to production Voice Mode

### Requirement: Cold and warm invocation use one authority
Both cold invocation and warm invocation SHALL open the intended durable conversation and submit through the same shared turn authority used by in-app text and voice.

#### Scenario: Cold Siri invocation
- **WHEN** Siri invokes the voice-agent action while Pod0 is not running
- **THEN** Pod0 launches, opens the intended durable conversation exactly once, and begins the turn through the shared authority

#### Scenario: Warm Shortcut invocation
- **WHEN** the Shortcut runs while Pod0 is active
- **THEN** the existing app process routes the request to the shared authority without mounting a second conversation or duplicating the turn

### Requirement: Fail-closed invocation feedback
An invocation that cannot reach the shared conversation, microphone, audio route, provider, or required approval surface SHALL return an explicit bounded failure and SHALL not claim that a turn started.

#### Scenario: Voice infrastructure is unavailable
- **WHEN** a cold or warm invocation cannot satisfy a required dependency
- **THEN** Siri or Shortcuts receives a truthful user-facing failure and no orphaned durable turn remains
