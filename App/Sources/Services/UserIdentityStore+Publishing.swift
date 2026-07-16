import CryptoKit
import Foundation

#if canImport(NMP)
import NMP
#endif

extension UserIdentityStore {
    func publishProfile(
        name: String,
        displayName: String,
        about: String,
        picture: String
    ) async throws -> SignedNostrEvent {
        let payload: [String: String] = [
            "name": name,
            "display_name": displayName,
            "about": about,
            "picture": picture,
        ]
        let data = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
        let content = String(data: data, encoding: .utf8) ?? "{}"
        let event = try await signAndPublish(NostrEventDraft(kind: 0, content: content))

        let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedDisplayName = displayName.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedAbout = about.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedPicture = picture.trimmingCharacters(in: .whitespacesAndNewlines)
        profileName = trimmedName.isEmpty ? nil : trimmedName
        profileDisplayName = trimmedDisplayName.isEmpty ? nil : trimmedDisplayName
        profileAbout = trimmedAbout.isEmpty ? nil : trimmedAbout
        profilePicture = trimmedPicture.isEmpty ? nil : trimmedPicture

        if let pubkey = publicKeyHex {
            let cachePayload: [String: String] = [
                "display_name": trimmedDisplayName,
                "name": trimmedName,
                "about": trimmedAbout,
                "picture": trimmedPicture,
            ]
            if let cacheData = try? JSONSerialization.data(withJSONObject: cachePayload) {
                UserDefaults.standard.set(cacheData, forKey: Self.kind0CachePrefix + pubkey)
            }
        }
        return event
    }

    func publishUserNote(_ note: Note, episodeCoord: String?) async throws -> SignedNostrEvent {
        var tags: [[String]] = [["t", "note"]]
        if let episodeCoord, !episodeCoord.isEmpty {
            tags.insert(["a", episodeCoord], at: 0)
        }
        return try await signAndPublish(
            NostrEventDraft(kind: 1, content: note.text, tags: tags)
        )
    }

    func publishUserClip(
        _ clip: Clip,
        episode: Episode? = nil,
        podcast: Podcast? = nil
    ) async throws -> SignedNostrEvent {
        var tags: [[String]] = []
        if let enclosureURL = episode?.enclosureURL {
            tags.append(["r", enclosureURL.absoluteString])
        }
        if let feedURL = podcast?.feedURL {
            tags.append(["r", feedURL.absoluteString])
        }
        if let guid = episode?.guid {
            tags.append([
                "i",
                "podcast:item:guid:\(guid)#t=\(clip.startMs / 1000),\(clip.endMs / 1000)",
            ])
        }
        tags.append(["context", clip.transcriptText])
        if let caption = clip.caption, !caption.isEmpty {
            tags.append(["alt", caption])
        }
        return try await signAndPublish(
            NostrEventDraft(kind: 9802, content: clip.transcriptText, tags: tags)
        )
    }

    func publishFeedbackNote(
        category: FeedbackCategory,
        body: String,
        parentEventID: String?,
        replyToPubkey: String?
    ) async throws -> SignedNostrEvent {
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
        return try await signAndPublish(
            NostrEventDraft(kind: 1, content: body.trimmed, tags: tags)
        )
    }

    private func signAndPublish(_ draft: NostrEventDraft) async throws -> SignedNostrEvent {
        #if canImport(NMP)
        guard let engine = nmpComposition?.engine else {
            throw UserIdentityError.nmpUnavailable
        }
        let event = try await signThroughNMP(draft)
        _ = try await engine.publish(
            WriteIntent(
                payload: .signed(
                    id: event.id,
                    pubkey: event.pubkey,
                    createdAt: UInt64(event.created_at),
                    kind: UInt16(event.kind),
                    tags: event.tags,
                    content: event.content,
                    sig: event.sig
                ),
                durability: .durable,
                routing: .authorOutbox
            )
        )
        return event
        #else
        throw UserIdentityError.nmpUnavailable
        #endif
    }

    func signThroughNMP(_ draft: NostrEventDraft) async throws -> SignedNostrEvent {
        #if canImport(NMP)
        guard let engine = nmpComposition?.engine else {
            throw UserIdentityError.nmpUnavailable
        }
        guard let createdAt = UInt64(exactly: draft.createdAt),
              let kind = UInt16(exactly: draft.kind) else {
            throw UserIdentityError.nmpUnavailable
        }
        let signed = try await engine.signEvent(
            NMPUnsignedEvent(
                createdAt: createdAt,
                kind: kind,
                tags: draft.tags,
                content: draft.content
            )
        )
        return SignedNostrEvent(
            id: signed.id,
            pubkey: signed.pubkey,
            created_at: Int(signed.createdAt),
            kind: Int(signed.kind),
            tags: signed.tags,
            content: signed.content,
            sig: signed.signature
        )
        #else
        throw UserIdentityError.nmpUnavailable
        #endif
    }

    func uploadProfilePhoto(_ data: Data, contentType: String) async throws -> URL {
        #if canImport(NMP)
        guard let engine = nmpComposition?.engine, let publicKeyHex else {
            throw UserIdentityError.noIdentity
        }
        let hash = Data(SHA256.hash(data: data)).hexString
        let now = UInt64(Date().timeIntervalSince1970)
        let draft = try blossomUploadAuthorizationDraft(
            authorPubkeyHex: publicKeyHex,
            blobSha256Hex: hash,
            createdAt: now,
            expiration: now + 300,
            description: "Upload profile photo"
        )
        let signed = try await engine.signEvent(draft.signRequest)
        let authorization = try BlossomAuthorization.validate(
            signedEvent: signed,
            verb: .upload,
            blobSha256Hex: hash,
            now: now
        )
        let descriptor = try await BlossomClient().upload(
            serverURL: BlossomUploader.defaultServer.absoluteString,
            blob: data,
            contentType: contentType,
            authorization: authorization
        )
        guard let url = URL(string: descriptor.url) else {
            throw UserIdentityError.nmpUnavailable
        }
        return url
        #else
        throw UserIdentityError.nmpUnavailable
        #endif
    }
}
