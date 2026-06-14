---
title: Nostr Remote Signer
slug: nostr-remote-signer
topic: nostr-protocol
summary: "Nostr event signing uses `NostrCredentialStore.privateKey()` + `NostrKeyPair(privateKeyHex:)` to build a `LocalKeySigner`"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-13
updated: 2026-06-14
verified: 2026-05-13
compiled-from: conversation
sources:
  - session:0f3f24f7-54de-49f8-b160-a92f735f6a00
  - session:16a9893c-f4c6-486d-ade2-e290ff0ca5d9
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
  - session:84c4d49c-d034-4d2e-8e1e-dfd5a4453b2d
  - session:02078283-91db-41b1-80f8-989daef628ac
  - session:f3b466c6-7791-44b3-b004-aae2066a9019
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:cf4c4d92-a662-4077-8787-9cfba26007a1
  - session:rollout-2026-05-17T17-57-58-019e3671-a863-7ab1-a96d-6ceb8b541971
---

# Nostr Remote Signer

## Signer Architecture

Nostr event signing uses `NostrCredentialStore.privateKey()` + `NostrKeyPair(privateKeyHex:)` to build a `LocalKeySigner`. (`NostrCredentialStore.privateKey()` is used for key access; `loadKeyPair()` does not exist.) NIP-46 remote signer support is now implemented; it prevents full deletion of Swift-side Nostr event signing because the kernel only holds the agent identity, not the user's nsec. Creating a private show does not require a Nostr signing key — when visibility is `.private`, `ownerPubkeyHex` falls back to an 'agent-private' sentinel value. LocalKeySigner is kept for signing in `NostrAgentResponder` and `LivePeerEventPublisher` because `core.publishPeerReply` deliberately omits a-tag copy-through from root events. All event signing in the app must route through the kernel rather than requiring the app to know how to sign events, making even agent artwork local signing a D13 violation. (Previously: Agent artwork uploads were kept on local signing because the agent key is always a locally-generated key.)

Both the Blossom upload auth and ShakeFeedback event call sites use kernel-mediated signing, not raw private key signing, making them D13-compliant. <!-- [^cf4c4-4] -->

The kernel kind:1 auto-responder (v1) restores the deleted NostrAgentResponder capability in-kernel: trusted inbound kind:1 → llm::complete_for_role reply → handle_publish_agent_note publish, with dedup via RespondedIds ring (MAX_RESPONDED_IDS=4096 VecDeque+HashSet), maxOutgoingTurnsPerRoot=10, wtd-end conversation gate, and no tool loop or ask coordinator. The responded_event_ids dedup set is bounded (RespondedIds ring with VecDeque for order + HashSet for O(1) membership, cap 4096, oldest eviction on overflow) and persists across process restarts and identity switches as a global/account-agnostic cache (fail-safe: cross-account carryover can only suppress, never over-reply). <!-- [^c1691-197] -->

<!-- citations: [^0f3f2-52] [^84c4d-12] [^02078-1] [^f3b46-15] [^14943-10] [^cf4c4-3] -->
## Early-Exit & Ended-Root Handling

The `nostrEndedRootIDs` early-exit block in `NostrAgentResponder.process()` has been removed, so subsequent messages on any root always reach the LLM instead of being silently dropped. The `nostrEndedRootIDs` state field, the `PeerConversationEndSink` protocol, its live and no-op implementations (`LivePeerConversationEndSink`, `NoOpPeerConversationEndSink`), the mock (`MockPeerConversationEndSink`), and all call sites have been fully removed from both app and test code. A `recordTurn()` call is added in the ended-root early-exit path of `NostrAgentResponder` so that messages sent after a conversation is genuinely ended (via turn cap or peer end signal) still appear in the conversation tab. <!-- [^16a98-2] -->

## Signed-Events Typed-Sidecar Decode

The signed_events typed-sidecar decode fix (PR #385) injects decoded FlatBuffer signed_events under v.projections["signed_events"] in nmp_app_podcast_decode_update_frame, un-breaking the NIP-46/active-account signing path that was silently dropping every signEventForReturn since v0.3.0. <!-- [^c1691-5] -->

## RemoteSigner Core

RemoteSignerClient accepts an optional `bunkerPubkeyHex` parameter; when nil, the subscription filter omits the authors field to allow responses from any signer. The `RemoteSignerTransportFactory` typealias accepts `bunkerPubkeyHex` as `String?` (was `String`). RemoteSigner provides a `finishNostrConnect(relayURL:)` method that opens a fresh transport, sets the conversation key, and calls only `get_public_key` — skipping the connect RPC to avoid duplicate `auth_url` challenges. The app must not include `relay.nsec.app` as a default NIP-46 pairing relay; QR signer pairing must use the app's configured Nostr relay instead.

<!-- citations: [^02078-2] [^rollo-165] -->
## NostrConnect Implementation

RemoteSigner+NostrConnect.swift implements the `nostrconnect` static factory, URI builder, inbound secret listener (`AsyncThrowingStream` + `withThrowingTaskGroup` racing a 5-minute timeout), and secret generation (`SecRandomCopyBytes` → 32 hex chars). The default nostrconnect relay is `wss://relay.primal.net`. The default nostrconnect permissions string is `sign_event:1,sign_event:6,sign_event:7,nip44_encrypt,nip44_decrypt`. The nostrconnect URI is built using `URLComponents`, which percent-encodes the relay URL as a query parameter value. The nostrconnect discovery flow does not send a connect RPC after receiving the inbound secret; the signer's `result==secret` response is the connect ack, and only `get_public_key` is called afterward. <!-- [^02078-3] -->

## NostrConnect Pairing Flow

UserIdentityStore+NIP46.swift provides `connectViaNostrConnect(relay:onURI:)` that orchestrates the full pairing flow using internal helpers (`_beginNostrConnect`, `_adoptNostrConnectSigner`, `_failNostrConnect`). Private-set properties (`loginError`, `remoteSignerState`) are mutated from extension files via internal `_`-prefixed helper methods defined in the main UserIdentityStore file. <!-- [^02078-4] -->

## NostrConnectView UI

NostrConnectView.swift displays a QR code, signer app detection buttons (Amber/Primal), and waiting/connected/error states driven by `remoteSignerState`. NostrConnectView detects installed signer apps via `UIApplication.shared.canOpenURL` for `KnownSigner` cases `.amber` (scheme: `nostrsigner`) and `.primal` (scheme: `primal`). `NostrConnectView.openSignerApp` appends `&callback=podcastr%3A%2F%2Fnip46` to the deep link URL so the signer app returns to the podcast app after approval. The Cancel toolbar button in NostrConnectView is hidden when the signer is already paired (`isPaired == true`), preventing accidental disconnection of a just-paired session. RemoteSignerView includes a 'Scan to connect' row with a navigation destination to NostrConnectView. Info.plist includes `LSApplicationQueriesSchemes` entries for `nostrsigner` and `primal` to enable `canOpenURL` detection on iOS 18+. <!-- [^02078-5] -->

## Android JNI Bindings

Android NIP-46 remote signer support requires new JNI wrappers in android.rs for three existing nmp-ffi symbols (nmp_app_signin_bunker, nmp_app_cancel_bunker_handshake, nmp_app_nostrconnect_uri) because Android's JNI surface did not export them. The Android NIP-46 remote signer screens must gate handshake completion on the transition to an external (remote-signer) active account (mode != local_key) rather than string-matching mode == bunker/nip46, because the identity projection only ever emits local_key or nip55. nmp_signer_broker_init must be called in Android's nativeNew; it was a latent omission that meant NIP-46 bunker handshakes could not function on Android despite the FFI symbols existing.

<!-- citations: [^c1691-411] [^c1691-421] [^c1691-439] [^c1691-456] -->
