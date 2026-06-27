# Scenario K1: Resolve the user's hex pubkey and prove `nak` can read the relay

## Goal
Establish the host-side relay-inspection harness that every other K-group NIP-84
scenario depends on. The F2 run was BLOCKED because the hex pubkey was copied
malformed ("94030fb0a1b47_8654df7a5714b") and `nak req` rejected it. This scenario
makes pubkey resolution deterministic by going npub → hex via `nak decode` (never
by reading a hex string off the UI), and proves a `nak req` round-trip succeeds.

## Prerequisites
- App past onboarding, signed in with a Nostr identity (A1 or A2).
- Host has `nak` on PATH (`/Users/pablofernandez/go/bin/nak`, `nak version` → ok).
- Network reachable from the host to `wss://relay.primal.net`.

## Steps
1. In the app: Settings → Account → Identity (A5). Copy the **npub** (bech32,
   starts `npub1…`) using the copy affordance — do NOT transcribe the hex shown in
   the UI by hand; UI rendering truncated/garbled it in the F2 run. *Screenshot.*
2. On the host, convert npub → hex deterministically:
   ```
   nak decode <npub1...>
   ```
   **Expected:** a single 64-character lowercase hex string (the `pubkey` field).
   Record it as `$HEX`. Sanity-check: `echo -n "$HEX" | wc -c` must print `64`.
3. Prove a relay round-trip with a broad filter (any kind from this author):
   ```
   nak req -a <HEX> -l 5 wss://relay.primal.net
   ```
   **Expected:** zero or more JSON events stream back with no error. Even an empty
   result (no events yet) is a PASS for the harness — the point is that the command
   parses the pubkey and the relay responds. A pubkey-format error is a FAIL.
4. Record `$HEX` in this scenario's Notes so K2–K6 can paste it verbatim.

## Acceptance Criteria
- `nak decode <npub>` yields exactly one 64-char hex pubkey.
- `nak req -a <HEX> ... relay.primal.net` runs without a pubkey-format/parse error
  (empty result set is acceptable).
- The hex value is recorded for downstream K scenarios.

## Known Issues / Watch Points
- NEVER use a hex value read off the iOS Identity screen — the F2 failure was a
  copy/render artifact. Always derive hex from the npub with `nak decode`.
- `nak decode` also accepts `nsec`/`note`/`naddr`; make sure you decoded the npub
  (public key), not a note id.
- If `nak req` hangs, add a relay timeout or Ctrl-C after ~5s; primal can be slow.
  A hang is a network/relay issue, not a scenario FAIL — note it and retry.

## Notes
