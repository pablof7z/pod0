import Foundation

// MARK: - User-identity publishing (slice B)
//
// Implements the wiring contract from
// `docs/spec/briefs/identity-05-synthesis.md` §5.
//
// ## Signing seam — kernel for local keys, Swift for bunkers
//
// kind:0/1/9802 signing now lives in the Rust kernel (`podcast.social.*`,
// see `apps/nmp-app-podcast/src/social_publish_handler.rs`). For a
// `.localKey` identity the key has been forwarded to the kernel (see
// `UserIdentityStore+Kernel.swift`), so these methods dispatch the build +
// sign + publish to the kernel.
//
// A `.remoteSigner` (NIP-46 bunker) identity keeps its key remote; the
// kernel's podcast-app `IdentityStore` has no `secret_hex` to sign with, so
// bunker publishing stays on the Swift NIP-46 path below until a kernel
// remote-sign seam exists (BACKLOG `social-bunker-signing-kernel`). The
// `.none` case self-heals to a generated local key, then takes the kernel
// path.
//
// The `author == .user` / `source != .agent` gate that keeps the user's
// identity off agent-authored artefacts lives UPSTREAM in
// `AppStateStore.addNote` / `addClip` — these methods only run for
// user-authored content, so the kernel path inherits the same property.
//
// Lives in a separate file purely to keep `UserIdentityStore.swift` under
// the 500-line hard cap defined in `AGENTS.md`.

extension UserIdentityStore {

    /// Synthesize the `SignedNostrEvent` callers expect when the actual
    /// sign happens kernel-side. The kernel dispatch is fire-and-forget
    /// (`DispatchResult`, not an event); every production call-site discards
    /// the return value, so this stub only satisfies the signature. The
    /// `pubkey` is the real active pubkey so any caller that does read it
    /// gets a truthful author.
    private func kernelDispatchedEventStub(kind: Int, content: String, tags: [[String]]) -> SignedNostrEvent {
        SignedNostrEvent(
            id: "",
            pubkey: publicKeyHex ?? "",
            created_at: Int(Date().timeIntervalSince1970),
            kind: kind,
            tags: tags,
            content: content,
            sig: ""
        )
    }

    /// Sign + publish a kind:0 metadata event with the supplied profile
    /// fields. Driven by the EditProfile flow:
    /// is driven by the EditProfile flow rather than auto-publish on first
    /// launch. The resulting event is fanned out across every relay in
    /// `FeedbackRelayClient.profileRelayURLs`; success is "at least one
    /// relay acked." Returns the signed event so callers can echo it into
    /// local profile state.
    func publishProfile(name: String, displayName: String, about: String, picture: String) async throws -> SignedNostrEvent {
        // Self-heal: a fresh user with no identity gets a kernel-generated
        // account dispatched here (the pubkey lands on the next snapshot tick).
        // The kernel signs with its active account — there is no Swift signer.
        try _ensureGeneratedKey()
        let payload: [String: String] = [
            "name": name,
            "display_name": displayName,
            "about": about,
            "picture": picture,
        ]
        let data = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
        let content = String(data: data, encoding: .utf8) ?? "{}"

        // Sign + publish kind:0 through the kernel (`podcast.social`). The
        // kernel signs with the active account — local nsec OR NIP-46 bunker —
        // so there is no Swift signing path here for either identity mode.
        dispatchToKernel(
            namespace: "podcast.social",
            body: [
                "op": "publish_profile",
                "name": name,
                "display_name": displayName,
                "about": about,
                "picture": picture,
            ]
        )
        let event = kernelDispatchedEventStub(kind: 0, content: content, tags: [])

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
        dispatchToKernel(namespace: "podcast.social", body: body)
        return kernelDispatchedEventStub(kind: 1, content: note.text, tags: [])
    }

    /// Sign + publish a user-authored clip as a kind:9802 highlight (NIP-84)
    /// with NIP-73 external content IDs for the podcast ecosystem.
    ///
    /// Passes typed fields to the `podcast.social` handler; the kernel
    /// assembles the NIP-73 / NIP-84 tag set (`r` enclosure + feed, `i` item
    /// guid with media-fragment time range, `context`, `alt`). Nostr tag
    /// semantics belong in the kernel, matching `LivePeerEventPublisher`'s
    /// "Swift passes typed values; Rust builds tags" convention (#355).
    ///
    /// `episode` and `podcast` are optional so callers that lack the
    /// resolved models can still publish a degraded-but-valid event — the
    /// kernel emits whichever tags the present fields support.
    func publishUserClip(
        _ clip: Clip,
        episode: Episode? = nil,
        podcast: Podcast? = nil
    ) async throws -> SignedNostrEvent {
        // Self-heal: a fresh user with no identity gets a kernel-generated
        // account dispatched here (the pubkey lands on the next snapshot tick).
        // The kernel signs with its active account — there is no Swift signer.
        try _ensureGeneratedKey()

        var body: [String: Any] = [
            "op": "publish_highlight",
            "content": clip.transcriptText,
        ]
        if let enclosureURL = episode?.enclosureURL {
            body["enclosure_url"] = enclosureURL.absoluteString
        }
        if let feedURL = podcast?.feedURL {
            body["feed_url"] = feedURL.absoluteString
        }
        if let guid = episode?.guid {
            body["item_guid"] = guid
            body["start_sec"] = clip.startMs / 1000
            body["end_sec"]   = clip.endMs   / 1000
        }
        if let caption = clip.caption, !caption.isEmpty {
            body["caption"] = caption
        }

        dispatchToKernel(namespace: "podcast.social", body: body)
        return kernelDispatchedEventStub(kind: 9802, content: clip.transcriptText, tags: [])
    }
}
