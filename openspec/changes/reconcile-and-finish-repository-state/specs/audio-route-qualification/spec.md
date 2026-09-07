## Purpose

Defines native audio-session events and physical-device evidence required for reliable playback and Voice Mode across real route changes and interruptions.

## ADDED Requirements

### Requirement: Typed interruption and route events
The native audio owner SHALL deliver typed events for interruption begin/end, route loss/change, media-services reset, and relevant foreground/background transitions to playback and Voice Mode consumers.

#### Scenario: A phone or Siri interruption ends
- **WHEN** the system reports that an interruption ended with permission to resume
- **THEN** the appropriate prior playback or voice state resumes once without duplicating audio or losing durable progress

#### Scenario: An output route disappears
- **WHEN** a wired, Bluetooth, or AirPlay route becomes unavailable
- **THEN** consumers receive the route-loss event and transition to the defined safe paused or rerouted state

### Requirement: AirPlay-compatible Voice Mode configuration
Voice Mode SHALL use a record/playback audio-session configuration that keeps AirPlay testable and SHALL not select the voice-chat mode that precludes the required route behavior.

#### Scenario: Voice Mode starts with an AirPlay-capable route
- **WHEN** the listener enters Voice Mode while AirPlay is available
- **THEN** the selected audio-session category, mode, and options preserve the supported AirPlay route choices

### Requirement: Physical-device qualification matrix
The authoritative release candidate SHALL pass wired disconnect, Bluetooth disconnect/reconnect, phone or Siri interruption/resume, background/foreground transition, and lock-screen control scenarios on a real supported iPhone.

#### Scenario: The full device matrix passes
- **WHEN** every required scenario is executed on a named device and OS build against the candidate SHA
- **THEN** results, timestamps, logs, and screenshots or recordings are captured and physical-device qualification may be marked complete

#### Scenario: Hardware is unavailable or one scenario fails
- **WHEN** the named device cannot be used or any required scenario is incomplete or unsuccessful
- **THEN** the candidate remains physically unqualified and downstream Siri enablement stays blocked
