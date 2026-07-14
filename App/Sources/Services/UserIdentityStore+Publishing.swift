import Foundation

// MARK: - User-identity publishing (slice B)
//
// Implements the wiring contract from
// `docs/spec/briefs/identity-05-synthesis.md` §5 — every method below
// mirrors the existing `publishFeedbackNote` shape: ensure a signer
// exists (auto-generate if needed), sign through the active `NostrSigner`,
// and publish through `FeedbackRelayClient.publish(..., authSigner:)`.
//
// Lives in a separate file purely to keep `UserIdentityStore.swift` under
// the 500-line hard cap defined in `AGENTS.md`.

extension UserIdentityStore {

    /// Sign + publish a kind:0 metadata event with the supplied profile
    /// fields. Mirrors the shape of `publishGeneratedProfileIfNeeded` but
    /// is driven by the EditProfile flow rather than auto-publish on first
    /// launch. The resulting event is fanned out across every relay in
    /// `FeedbackRelayClient.profileRelayURLs`; success is "at least one
    /// relay acked." Returns the signed event so callers can echo it into
    /// local profile state.
    func publishProfile(name: String, displayName: String, about: String, picture: String) async throws -> SignedNostrEvent {
        if signer == nil {
            try _ensureGeneratedKey()
        }
        guard let signer else { throw UserIdentityError.noIdentity }
        let payload: [String: String] = [
            "name": name,
            "display_name": displayName,
            "about": about,
            "picture": picture,
        ]
        let data = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
        let content = String(data: data, encoding: .utf8) ?? "{}"
        let event = try await signer.sign(NostrEventDraft(kind: 0, content: content))
        var lastError: Error?
        var anyAck = false
        for relayURL in FeedbackRelayClient.profileRelayURLs {
            let client = FeedbackRelayClient(relayURL: relayURL)
            do {
                try await client.publish(event, authSigner: signer)
                anyAck = true
            } catch {
                lastError = error
                continue
            }
        }
        if !anyAck, let lastError {
            throw lastError
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

        return event
    }

    /// Sign + publish a user-authored note as a kind:1 text note. Matches
    /// the row "Notes (user)" in `identity-05-synthesis.md` §5.3.
    /// `episodeCoord` is the `30311:<author>:<id>` reference (or whatever
    /// shape the episode coordinate adopts) — passed through verbatim into
    /// an `["a", episodeCoord]` tag when present. Today no call-site has
    /// an episode coord to pass in, so the tag is omitted; future episode-
    /// anchored notes will populate it.
    func publishUserNote(_ note: Note, episodeCoord: String?) async throws -> SignedNostrEvent {
        if signer == nil {
            try _ensureGeneratedKey()
        }
        guard let signer else { throw UserIdentityError.noIdentity }
        var tags: [[String]] = [["t", "note"]]
        if let episodeCoord, !episodeCoord.isEmpty {
            tags.insert(["a", episodeCoord], at: 0)
        }
        let event = try await signer.sign(NostrEventDraft(kind: 1, content: note.text, tags: tags))
        try await FeedbackRelayClient().publish(event, authSigner: signer)
        return event
    }

    /// Sign + publish a user-authored clip as a kind:9802 highlight (NIP-84)
    /// with NIP-73 external content IDs for the podcast ecosystem.
    ///
    /// Tag structure:
    /// - `["r", enclosureURL]` — NIP-84 source URL (the audio file).
    /// - `["r", feedURL]` — podcast feed reference.
    /// - `["i", "podcast:item:guid:<guid>#t=<start>,<end>"]` — NIP-73
    ///   external content ID with media-fragment time offset (seconds).
    /// - `["context", transcriptText]` — NIP-84 surrounding context.
    /// - `["alt", caption]` — human-readable description when present.
    ///
    /// `episode` and `podcast` are optional so callers that lack the
    /// resolved models can still publish a degraded-but-valid event.
    func publishUserClip(
        _ clip: Clip,
        episode: Episode? = nil,
        podcast: Podcast? = nil
    ) async throws -> SignedNostrEvent {
        if signer == nil {
            try _ensureGeneratedKey()
        }
        guard let signer else { throw UserIdentityError.noIdentity }

        var tags: [[String]] = []

        // NIP-84: source URL — the audio enclosure.
        if let enclosureURL = episode?.enclosureURL {
            tags.append(["r", enclosureURL.absoluteString])
        }

        // NIP-73: podcast feed reference (show level).
        if let feedURL = podcast?.feedURL {
            tags.append(["r", feedURL.absoluteString])
        }

        // NIP-73: episode external content ID with time-fragment offset.
        if let guid = episode?.guid {
            let startSec = clip.startMs / 1000
            let endSec   = clip.endMs   / 1000
            tags.append(["i", "podcast:item:guid:\(guid)#t=\(startSec),\(endSec)"])
        }

        // NIP-84: surrounding context.
        tags.append(["context", clip.transcriptText])

        if let caption = clip.caption, !caption.isEmpty {
            tags.append(["alt", caption])
        }

        let event = try await signer.sign(NostrEventDraft(kind: 9802, content: clip.transcriptText, tags: tags))
        try await FeedbackRelayClient().publish(event, authSigner: signer)
        return event
    }

    /// Sign + publish a structured feedback note (kind:1) anchored to the
    /// project's NIP-72 community coordinate. Auto-generates a key if no
    /// identity is active (same self-heal as the rest of the publishing surface).
    func publishFeedbackNote(
        category: FeedbackCategory,
        body: String,
        parentEventID: String?,
        replyToPubkey: String?
    ) async throws -> SignedNostrEvent {
        if signer == nil {
            try _ensureGeneratedKey()
        }
        guard let signer else { throw UserIdentityError.noIdentity }
        var tags: [[String]] = [
            ["a", FeedbackRelayClient.projectCoordinate],
            ["t", category.tagValue],
            ["-"],
        ]
        if let parentEventID {
            tags.append(["e", parentEventID, "", "root"])
        }
        if let replyToPubkey {
            tags.append(["p", replyToPubkey])
        }
        let event = try await signer.sign(NostrEventDraft(kind: 1, content: body.trimmed, tags: tags))
        try await FeedbackRelayClient().publish(event, authSigner: signer)
        return event
    }
}
