---
title: NIP-46 Message
slug: nip46-message
topic: nostr-protocol
summary: Nip46Message declares nip04_encrypt/decrypt and nip44_encrypt/decrypt enum cases that are not used for MVP.
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
  - session:f3b466c6-7791-44b3-b004-aae2066a9019
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# NIP-46 Message

## Encryption Cases

NIP-44 encryption is handled entirely by the kernel; Swift never holds a RemoteSigner or performs NIP-44 encryption, making the NIP-04 stub irrelevant on the Swift side. (Previously: NIP-04 encrypt/decrypt is stubbed as unsupported because NIP-44 v2 is used throughout, superseded — see nostr-rust-ffi.)

<!-- citations: [^0f3f2-49] [^f3b46-1] -->

## Pairing Flow

NIP-46 (nostrConnect/bunker) pairing is handled entirely by the kernel (nmp_app_signin_bunker); Swift never holds a RemoteSigner, replacing the previous two-call start/await pattern. (Previously: NIP-46 uses a two-call pattern (start then await) so that start synchronously returns a URI for QR display and await blocks for pairing, superseded — see nostr-rust-ffi.) The nostrconnect URI is built using `URLComponents`, which percent-encodes the relay URL as a query parameter value. (Previously: The nostrconnect:// URI is hand-rolled because the SDK's NostrConnectURI::Display for the Client variant does not include the secret parameter, superseded — see nostr-remote-signer.) NIP-46 (nostrConnect/bunker) pairing is handled entirely by the kernel (nmp_app_signin_bunker); both nostrconnect:// and bunker:// pairing flows are supported. (Previously: Only nostrconnect:// pairing is implemented; bunker:// paste flow is not in scope. <!--  -->, superseded — see nostr-rust-ffi.)

## Replay Resilience

NIP-46 (nostrConnect/bunker) pairing is handled entirely by the kernel (nmp_app_signin_bunker); Swift never holds a RemoteSigner, so the .since() subscription filter for iOS suspension handling is now a kernel-internal concern. (Previously: NIP-46 start uses .since(Timestamp::now()) rather than .limit(0) so the connect response is replayed if iOS suspends the app between displaying the URI and signer approval. <!--  -->, superseded — see nostr-rust-ffi.)

## Signer Registration

nip46_await_signer calls client.set_signer(signer).await directly, not through runtime().set_signer() which uses block_on, to avoid a same-runtime panic. <!-- [^f3b46-4] -->

## Error Handling

RemoteSigner provides a `finishNostrConnect(relayURL:)` method that opens a fresh transport, sets the conversation key, and calls only `get_public_key` — skipping the connect RPC to avoid duplicate `auth_url` challenges. (Previously: No auth_url handler exists for NIP-46; if a signer demands a browser flow, a clear error is reported rather than silently failing. <!--  -->, superseded — see nostr-remote-signer.)

## State Management

PendingNip46 state is in-memory only; if the user kills the app mid-flow, they re-pair from scratch. <!-- [^f3b46-6] -->

## Testing

The URI validation tests in Nip46UriValidationTest exercise test-file-local helpers, not the production screen's inline checks, so they validate a parallel reimplementation rather than the shipped code path. <!-- [^c1691-416] -->
