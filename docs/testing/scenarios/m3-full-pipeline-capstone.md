# Scenario M3: Full pipeline capstone — provider → transcript → agent clip → relay

## Goal
A single end-to-end run chaining every critical area, used as the suite's smoke
test for "the headline product loop works": configure providers, transcribe, ask
the agent to find and clip the key moment, and verify the published NIP-84 event on
the relay is semantically bounded. If M3 passes, K/L are individually verifiable; if
M3 fails, it localizes WHICH stage broke.

## Prerequisites
- K1 done (`$HEX` recorded). Fresh-ish app state (do NOT seed; real network paths).

## Steps
1. **Providers:** Configure + validate OpenRouter (I2, key
   key from `~/.tenex/providers.json` → `openrouter.apiKey`) and
   configure + connect Ollama (L2). Select Whisper for Speech (K5) and
   `deepseek-v4-flash:cloud` for the Agent role (L1). *Screenshot of both configured.*
2. **Transcript:** Pick a transcribed, indexed Daily episode (E4) OR transcribe a
   short no-transcript downloaded episode via Whisper (K5). Confirm coherent
   transcript text. *Screenshot.*
3. **Agent clip:** Open the agent chat in that episode's context (L4 step 2). Ask:
   *"Find the most insightful moment in this episode and clip it, then tell me why
   you chose it."* **Expected:** a grounded rationale + a tool-batch "Agent ran N
   actions" creating a clip. *Screenshot.*
4. **In-app verify:** Open Clippings; confirm the Agent-badged clip; read its
   transcript excerpt; confirm sentence/idea-aligned boundaries (L5 step 4). *Screenshot.*
5. **Manual control + relay:** Also create ONE manual clip from a transcript segment
   with caption `M3-<random>` (K3), Save. On the host:
   ```
   nak req -k 9802 -a <HEX> -s <T0> -l 10 wss://relay.primal.net
   ```
   **Expected:** the MANUAL clip's kind:9802 event is present (matching caption,
   `context`/`content` = transcript text, `i`-tag `#t=<int>,<int>` matching the
   range, no a-tag); the AGENT clip's event is ABSENT (K7 contract). *Paste JSON.*
6. **Grounding cross-check:** Search the Transcripts index (E4/L7) for a phrase from
   the agent's chosen moment; confirm it's found in the same episode. *Screenshot.*

## Acceptance Criteria
- All providers configure and validate; transcript is produced/available.
- The agent returns a grounded rationale AND creates an Agent-badged clip with
  sentence/idea-aligned boundaries.
- A manual clip publishes a correct kind:9802 (i-tag, context==content, no a-tag) to
  `relay.primal.net`; the agent clip does NOT auto-publish (K7).
- The agent's chosen moment is corroborated by transcript search (L7).

## Known Issues / Watch Points
- This chains many surfaces; if it fails, isolate the stage and fall back to the
  focused scenario (provider→L1/L2/K5; transcript→K5; agent clip→L5; relay→K2;
  grounding→L7) to localize the break.
- Use `-s <T0>` and unique caption markers to avoid matching stale events.
- Agent tool-use depends on a tool-capable Agent (Thinking) model — if no clip is
  created, set the Thinking role (L1) and retry before failing the capstone.
- Sim mic/voice is out of scope here (this is the text/clip loop); voice is M1/M2.

## Notes
