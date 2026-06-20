import Foundation

// MARK: - User-identity publishing (slice B)
//
// Implements the wiring contract from
// `docs/spec/briefs/identity-05-synthesis.md` §5.
//
// ## Signing seam — ALL paths through the kernel
//
// kind:0/1/9802 signing lives entirely in the Rust kernel (`podcast.social.*`,
// see `apps/nmp-app-podcast/src/social_publish_handler.rs`). For a
// `.localKey` identity the kernel signs with the persisted local nsec; for a
// `.remoteSigner` (NIP-46 bunker) identity the kernel parks the sign op on
// its `PendingSign` queue and resolves it via the signer broker round-trip
// (see `nmp-core`'s `actor/pending_sign.rs` + `commands/publish.rs`
// `sign_active_nonblocking`). There is NO Swift signing path for either
// identity kind; this file contains no NIP-46 code. The `.none` case
// self-heals to a generated local key via `_ensureGeneratedKey`, then takes
// the kernel path.
//
// The `author == .user` gate that keeps the user's identity off agent-authored
// notes lives UPSTREAM in `AppStateStore.addNote`; this file only runs for
// user-authored note content, so the kernel path inherits the same property.
//
// Lives in a separate file purely to keep `UserIdentityStore.swift` under
// the 500-line hard cap defined in `AGENTS.md`.

extension UserIdentityStore {

    /// Dispatch a kind:0 metadata event to the Rust kernel for signing and
    /// relay publish. The kernel signs with the active account (local nsec OR
    /// NIP-46 bunker) and routes via NIP-65 outbox — Swift never touches
    /// signing, event id, sig, or created_at.
    ///
    /// The dispatch is fire-and-forget: the kernel enqueues the sign op and
    /// returns "queued"; no signed event id or relay acknowledgement is
    /// synchronously available. Swift updates local profile state immediately
    /// so the UI reflects the new fields without a relay round-trip.
    ///
    /// NOTE: A future projection could surface the kernel's confirmed event
    /// id/sig back to Swift (tracked in docs/BACKLOG.md). Until then callers
    /// must not treat successful dispatch as relay confirmation.
    func publishProfile(name: String, displayName: String, about: String, picture: String) async throws {
        // Self-heal: a fresh user with no identity gets a kernel-generated
        // account dispatched here (the pubkey lands on the next snapshot tick).
        // The kernel signs with its active account — there is no Swift signer.
        try _ensureGeneratedKey()

        // Sign + publish kind:0 through the kernel (`podcast.social`). The
        // kernel signs with the active account — local nsec OR NIP-46 bunker —
        // so there is no Swift signing path here for either identity mode.
        //
        // A synchronous `.failure` (e.g. no active account, no kernel) is a
        // real rejection: throw BEFORE touching local state so the caller's
        // catch path shows the error and does NOT advance to "update sent."
        // On `.accepted` the kernel has enqueued the op; local state then
        // updates optimistically (fire-and-forget for the relay leg).
        let dispatchResult = dispatchToKernel(
            namespace: "podcast.social",
            body: [
                "op": "publish_profile",
                "name": name,
                "display_name": displayName,
                "about": about,
                "picture": picture,
            ]
        )
        if case let .failure(message) = dispatchResult {
            throw UserIdentityError.dispatchRejected(message)
        }

        // Update local state immediately so the UI reflects the new profile
        // without waiting for a relay round-trip on next launch.
        let trimmedName        = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedDisplayName = displayName.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedAbout       = about.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedPicture     = picture.trimmingCharacters(in: .whitespacesAndNewlines)
        profileName        = trimmedName.isEmpty        ? nil : trimmedName
        profileDisplayName = trimmedDisplayName.isEmpty ? nil : trimmedDisplayName
        profileAbout       = trimmedAbout.isEmpty       ? nil : trimmedAbout
        profilePicture     = trimmedPicture.isEmpty     ? nil : trimmedPicture

        if let pubkey = publicKeyHex {
            let cachePayload: [String: String] = [
                "display_name": trimmedDisplayName,
                "name":         trimmedName,
                "about":        trimmedAbout,
                "picture":      trimmedPicture,
            ]
            if let cacheData = try? JSONSerialization.data(withJSONObject: cachePayload) {
                UserDefaults.standard.set(cacheData, forKey: Self.kind0CachePrefix + pubkey)
            }
        }
    }

    /// Dispatch a user-authored kind:1 text note to the Rust kernel for
    /// signing and relay publish. Matches the row "Notes (user)" in
    /// `identity-05-synthesis.md` §5.3. Fire-and-forget — the kernel
    /// owns signing, event id, sig, created_at, and relay outcome.
    ///
    /// `episodeCoord` is the `30311:<author>:<id>` reference (or whatever
    /// shape the episode coordinate adopts) — passed through verbatim into
    /// an `["a", episodeCoord]` tag when present. Today no call-site has
    /// an episode coord to pass in, so the tag is omitted; future episode-
    /// anchored notes will populate it.
    func publishUserNote(_ note: Note, episodeCoord: String?) async throws {
        // Self-heal: a fresh user with no identity gets a kernel-generated
        // account dispatched here (the pubkey lands on the next snapshot tick).
        // The kernel signs with its active account — there is no Swift signer.
        try _ensureGeneratedKey()
        // Pass typed fields; the kernel builds the NIP tags (`["t","note"]`
        // plus an optional `["a", episode_coord]`) — Nostr tag semantics live
        // in the kernel, matching `LivePeerEventPublisher`'s convention (#355).
        var body: [String: Any] = ["op": "publish_note", "content": note.text]
        if let episodeCoord, !episodeCoord.isEmpty {
            body["episode_coord"] = episodeCoord
        }
        // A synchronous `.failure` must surface as a thrown error so callers
        // do not silently treat a rejected note as a success.
        let dispatchResult = dispatchToKernel(namespace: "podcast.social", body: body)
        if case let .failure(message) = dispatchResult {
            throw UserIdentityError.dispatchRejected(message)
        }
    }

}
