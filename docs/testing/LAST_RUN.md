# Last Test Run

- **Date:** 2026-06-24
- **Build:** main branch (post tuist generate)
- **Simulator:** podcast-iter iOS 26.5
- **Results:** 10 PASS / 4 PARTIAL / 3 FAIL / 15 BLOCKED (56 total)

## Results by Scenario

### A — Onboarding & Identity

| ID | Scenario | Result |
|----|----------|--------|
| A1 | Fresh install / generate keypair / onboarding | PASS |
| A2 | Import existing nsec private key | BLOCKED (Re-test Attempt) |
| A3 | Set display name, username, about, avatar | PASS |
| A4 | NIP-46 remote signer (bunker URI / QR) | BLOCKED |
| A5 | Account details, copy keys, relay config | PARTIAL |

### B — Podcast Discovery & Search

| ID | Scenario | Result |
|----|----------|--------|
| B1 | Search podcasts by keyword | (not run) |
| B2 | Add show by RSS URL | PARTIAL |
| B3 | Discover Nostr-published podcast (NIP-F4) | BLOCKED |
| B4 | Browse Home recommended / trending | PASS |
| B5 | Subscribe to a podcast | PASS |
| B6 | Unsubscribe (keep history) | PASS |

### C — Library & Episode Management

| ID | Scenario | Result |
|----|----------|--------|
| C1 | View subscribed podcasts grid/list | PARTIAL |
| C2 | Open show, browse episodes, search within show | PASS |
| C3 | Download episode, progress → downloaded → remove | PASS |
| C4 | Mark episode played/unplayed | BLOCKED |
| C5 | Episode detail view | BLOCKED |

### D — Playback

| ID | Scenario | Result |
|----|----------|--------|
| D1 | Play, pause, resume; mini-player + full player | BLOCKED |
| D2 | Skip forward/back by interval; long-press chapter jump | BLOCKED |
| D3 | Seek via scrubber drag and timeline tap | BLOCKED |
| D4 | Change playback speed | PARTIAL |
| D5 | Set and clear sleep timer | PASS |
| D6 | Now Playing / Control Center / lock-screen transport | BLOCKED |
| D7 | Queue / Up Next: add, reorder, remove, auto-play | BLOCKED |
| D8 | Chapter navigation; tap to seek | PASS |
| D9 | Capture 30-second AutoSnip clip | BLOCKED |
| D10 | Pre-roll/ad skip + auto-skip-ads | BLOCKED |

### E — Transcripts

| ID | Scenario | Result |
|----|----------|--------|
| E1 | View publisher-supplied transcript synced to playback | BLOCKED |
| E2 | Trigger AI (OpenRouter Whisper) transcription | BLOCKED |
| E3 | Tap transcript segment to seek playback | FAIL |
| E4 | Search within transcript / kernel knowledge search | PASS |

### F — NIP-84 Highlights (Clippings)

| ID | Scenario | Result |
|----|----------|--------|
| F1 | Create a clipping from transcript segment | BLOCKED |
| F2 | Verify NIP-84 metadata (a-tag, alt, context) | BLOCKED |
| F3 | View clippings (Today / This Week / Earlier buckets) | PASS |
| F4 | Share a clipping | PASS |
| F5 | Delete a clipping via context menu / swipe | PASS |

### G — AI Agent Interaction

| ID | Scenario | Result |
|----|----------|--------|
| G1 | Configure Ollama provider + select model | PARTIAL |
| G2 | Open agent chat; composer / history / new conversation | BLOCKED |
| G3 | Ask agent question about episode | BLOCKED |
| G4 | Ask agent to suggest/create highlight | (not run) |
| G5 | Voice mode (orb states, talk/stop/switch-to-text) | BLOCKED |

### H — Social / Nostr Features

| ID | Scenario | Result |
|----|----------|--------|
| H1 | Follow another Nostr user by pubkey | PASS |
| H2 | View friend detail / listening activity | PASS |
| H3 | Share episode (deeplink / system share / quote share) | BLOCKED |
| H4 | NIP-F4 publishing (author claim kind:10064) | BLOCKED |
| H5 | Episode comments + Feedback compose/thread | PASS |

### I — Settings

| ID | Scenario | Result |
|----|----------|--------|
| I1 | Configure AI providers (Ollama + model selection) | PARTIAL |
| I2 | Configure OpenRouter + enable Whisper fallback | PARTIAL |
| I3 | Playback settings: speed, skip intervals, auto-mark | PARTIAL |
| I4 | Storage/downloads management, data export, clear-all | PASS |
| I5 | Notification permission + per-show / new-episode toggles | PASS |

### J — Edge Cases & Regression

| ID | Scenario | Result |
|----|----------|--------|
| J1 | Offline behavior: play downloaded, network failure handling | (not run) |
| J2 | Background playback; audio continues; resume on foreground | FAIL |
| J3 | Very long episode (>3h): time formatting, seeking, transcript | BLOCKED |
| J4 | Episode with no transcript: generate affordance / empty state | BLOCKED |
| J5 | Network errors during search / subscribe; no crash | BLOCKED |
| J6 | Resume playback position after kill and relaunch | FAIL |
