---
title: Nostr Rust FFI
slug: nostr-rust-ffi
topic: nostr-protocol
summary: All ad-hoc Nostr code is moved to the Rust side, exclusively event-driven with no polling, using the nostr-sdk in Rust
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-15
updated: 2026-06-13
verified: 2026-05-15
compiled-from: conversation
sources:
  - session:f3b466c6-7791-44b3-b004-aae2066a9019
  - session:8e07824e-448c-4122-8a44-23c34c83b826
  - session:10228378-5073-48b1-9cd7-25b4834f2bac
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:8bfa1b91-b40c-44b3-acb9-245b36f4c841
  - session:a6320d4d-f2c8-4a8b-a21a-d71f5af73509
  - session:c43d5e77-d667-4e71-a574-47aaab5b6a7a
  - session:55bedfc3-dd9e-4b1c-b7d7-cea0c699d4d1
  - session:04b5f843-fdbe-4aa1-ae41-6770eac82957
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
  - session:38f8143c-c90d-49e3-a8fa-8d5ca17ac319
  - session:rollout-2026-05-17T17-57-58-019e3671-a863-7ab1-a96d-6ceb8b541971
  - session:rollout-2026-05-25T22-38-52-019e60a5-b1f5-7883-b0d3-a8d1826b1709
---

# Nostr Rust FFI

## Architecture Overview

All ad-hoc Nostr code is moved to the Rust side, exclusively event-driven with no polling, using the nostr-sdk in Rust. The architecture shape is Rust action modules plus Rust state/projections, with Swift as renderer/capability executor. All business logic must be Rust-owned per NMP guidelines, including Nostr event signing, triage, briefing composition, and publishing. The FFI surface is per-feature methods on PodcastrCore rather than a full TEA dispatch(AppAction) wrapper, avoiding costly Swift restructuring with no protocol benefit. The FFI ABI between Rust and Swift must match: the Rust export signature and the Swift header declaration must have the same parameter count. The ../highlighter project serves as reference code for the FFI approach. The app connects to Nostr relays once and lets NMP drive all relay routing automatically; the app must not specify relay URLs at publish or subscribe time. No WebSocket awareness (URLSessionWebSocketTask, direct relay connections) belongs in Swift; all Nostr relay communication is NMP's responsibility. The shared NDK instance is the sole mechanism for relay management; NostrPodcastDiscoveryService is the only component that directly calls addRelay() and connect() on it (to add NIP-F4 discovery relays on demand), and there are no secondary NDK instantiations or rogue ndk.connect() calls. Relay edits don't survive app restart because sidecar persistence (relay_config.rs) is locked inside NmpAppBuilder::start(), which the podcast app doesn't use. Relay publishing is fully owned by NMP; the podcast app does not own republishing or any queue semantics for it.

Swift passes only semantic data (recipient, root, content, channel anchors) to the kernel; Rust constructs all NIP-10 tags and Nostr event structure. NIP-73/84 tag construction for clip/note publishing (issue #355) is performed in the Rust kernel handlers (handle_publish_note, handle_publish_highlight) from typed fields, not from pre-built tag arrays passed by Swift. The Rust SocialAction enum for publish actions carries typed fields (content, clip_start_ms, clip_end_ms, episode_id, podcast_id, comment_url) rather than a pre-built tags dictionary. Subscribing to Nostr events uses EnsureInterest / push_interest with ViewDependencies; NMP routes through configured relays automatically, no URL specification needed. Comments subscribe via push_interest + CommentsObserver; agent notes subscribe via push_interest + AgentNotesObserver — both use NMP's relay pool, not iOS WebSockets. Profile fetch (kind:0) uses NMP's nmp_app_claim_profile kernel action instead of Swift WebSocket connections. NIP-46 (nostrConnect/bunker) pairing is handled entirely by the kernel (nmp_app_signin_bunker); Swift never holds a RemoteSigner or performs NIP-44 encryption. The kernel's publish_unsigned_event → sign_active_nonblocking path handles both local-keys (sync sign) and NIP-46 bunker (parked PendingSign) transparently; no Swift NIP-46 signing branch ever existed, and the false Swift header comment has been corrected. All Swift Nostr signing/crypto/key code is deleted: Nip46/ directory (RemoteSigner, Nip44, ChaCha20), NostrKeyPair, LocalKeySigner, the WebSocket relay stack, KernelSigner, the NostrSigner protocol, NostrEventDraft, BlossomUploader, NostrPendingApproval, NostrApprovalPresenter, and the nostrPendingApprovals field. NostrSignerError is retained because it is still used by KernelBridge.swift and SignedEventsRegistryTests.swift. NostrKeyPair.generate() for local-key account creation is replaced by the kernel's nmp_app_create_new_account; the kernel owns all key material. Blossom uploads (avatar and artwork) route through the upstream nmp.blossom.upload typed action, which handles sign+transport via SignEventForAccount (supporting both local nsec and NIP-46 bunker); Swift awaits the BlobDescriptor.url from action_results, and BlossomUploader.swift is deleted. Grep for KernelSigner, NostrSigner protocol, NostrEventDraft, NostrPendingApproval, NostrApprovalPresenter, nostrPendingApprovals, and BlossomUploader returns empty.

Nostr profile settings changes are dispatched as a single atomic set_nostr_profile op to the Rust kernel, not as three separate ops. (Previously: set_nostr_profile_name, set_nostr_profile_about, and set_nostr_profile_picture were dispatched separately, which silently failed deserialization and broke cross-device iCloud sync.)

Mark-played-at-end policy is delegated to the Rust kernel; the delete-after-played setting is stored/projected in Rust but consumed by nothing yet (BACKLOG item). Agent prompt inventory/filter/cap policy moved to the Rust kernel as an AgentContextSnapshot projection; Swift renders the pre-chosen lists into prompt strings.

Feedback thread reduction (issue #354) is performed in the Rust kernel as a typed feedback_threads projection; Swift's FeedbackStore consumes the projection via a FeedbackThreadDTO/FeedbackReplyDTO rather than reducing raw Nostr events client-side. The SignedNostrEvent struct is retained (used by signing/Blossom/publishing); only its feedback-specific tag-parsing extension and the buildThreads/FeedbackMetadata/asSignedEvent reducer are deleted from Swift.

Legacy pre-#215 agent episodes are backfilled into the Rust kernel on first launch via backfillSyntheticEpisodes, gated by a UserDefaults flag for one-shot crash-retry semantics.

The agent responder migration plan sequences: (1) kill NostrEventPublisher/LivePeerEventPublisher (kernel dispatch), (2) kill NostrProfileFetcher (EnsureInterest kind:0), (3) kill NostrThreadFetcher (EnsureInterest kind:1 one-shot), (4) move the LLM responder loop to Rust.

<!-- citations: [^14943-12] [^14943-13] [^14943-14] [^f3b46-16] [^10228-1] [^14943-11] [^c43d5-8] [^55bed-9] [^04b5f-6] [^rollo-215] [^c1691-185] [^c1691-212] [^c1691-233] [^c1691-289] -->
## Crate Dependencies

The Rust crate uses nostr-sdk 0.44.1 (current stable) with UniFFI 0.29. <!-- [^f3b46-17] -->


ios-shake-feedback uses swift-secp256k1's 0.19.0 secp256k1 module (the P256K namespace was ported away) so it shares the same pinned version as NDKSwift. NDKSwift is pinned to branch master at commit b0846731f89831a88b690ced067c0780dbf08dc0. <!-- [^8e078-1] -->

The cdylib crate type must remain in the Rust build for Android .so output, even though it also produces a .dylib that would cause DYLD failures on iOS if linked via -l flag. <!-- [^8bfa1-3] -->

PR #246 (Blossom + feedback auth through kernel) is the active follow-up for the deferred app-Rust signing and degraded uploads; it should be left open. <!-- [^c43d5-9] -->

ffi_guard wraps every extern C and JNI entry with a lazy fallback (impl FnOnce() -> T) that catches panics and returns a sentinel instead of aborting, with site-logging on the Err arm; panic=abort is explicitly rejected because it would nullify upstream catch_unwind. <!-- [^c1691-28] -->

The NDK cache path must be a directory, not a file path ending in `ndk.db`; NDKNostrDBCache construction must not silently fail. NDKSwift must install subscription event/EOSE callbacks before sending the relay REQ, so that fast relays cannot return EVENT+EOSE before the app has a handler attached. The NDK subscription collector must preserve events already yielded by the NDK stream when a timeout or EOSE condition fires, rather than discarding them and returning an empty array. <!-- [^rollo-167] -->
## App Boot Sequence

The Rust signer must be loaded from NostrCredentialStore.privateKey() at app boot; otherwise episode-publish coordinates point at shows the user does not own. bootstrapNostrSession is wired at app boot to load the Keychain private key into the Rust core; without it, every signed publish silently fails. installNostrAppObservers is called at app boot so signer and relay-status deltas reach AppState. <!-- [^f3b46-18] -->

## Lint and Compilation Policy

Timestamp::as_u64 deprecation warnings are replaced with as_secs (3 occurrences in podcast_discovery.rs and podcast_publisher.rs). PR #401 fixed three invalid Rust numeric literal suffixes (0jlong, 0jint, -1jint) introduced by ffi_guard in android.rs and added a cargo check --target aarch64-linux-android CI job to prevent future invisible Android-only Rust breakage.

<!-- citations: [^f3b46-19] [^c1691-60] -->
## Validation Milestone

The full cutover compiles and passes smoke test on iOS Simulator with the Rust static lib loaded, PodcastrCoreBridge initialized, and NostrRelayService.start() running the new Swift wrapper code with zero crashes.

PR #368 makes Android call bridge.setDataDir(filesDir) before bridge.start(), causing the Rust kernel to load podcasts.json, identity, queue, and settings from disk, surviving process restart.

PR #371 adds PlatformWidgetContractTests that decode a real Rust-emitted PodcastUpdate-with-widget JSON fixture through the bridge seam, asserting both widget and library survive, plus guards that a plain decoder must throw on the fixture.

<!-- citations: [^f3b46-20] [^a6320-2] [^38f81-8] -->
## Relay Diagnostics

Relay diagnostics views (NetworkingSettingsView, RelayDetailView) subscribe reactively to ndk.relayChanges (AsyncStream<NDKPoolChangeEvent>) instead of using polling loops, and rebuild only on pool events (relayAdded, relayRemoved, relayConnected, relayDisconnected). The refreshedAt property and the 'Last refresh' UI row are removed from the relay diagnostics model and views entirely. <!-- [^10228-2] -->

## Relay Connection

NostrPodcastDiscoveryService.ensureRelayConnected() uses relay.stateStream raced against a 3-second timeout instead of a 200ms polling loop, leveraging that stateStream emits the current state immediately and future changes. <!-- [^10228-3] -->

The connected relay count must not claim success when zero sockets are actually connected. <!-- [^rollo-166] -->
