import CryptoKit
import Foundation
import os.log

// MARK: - NostrPodcastPublisher
//
// Publishes NIP-74 (kind:30074 podcast show, kind:30075 podcast episode) addressable
// events signed by the agent's key. Both kinds are parameterised replaceable events
// (NIP-33): each `d` tag uniquely identifies the show / episode across updates.
//
// Audio, chapters, and transcripts must be uploaded to Blossom before calling
// `publishEpisode` — the caller passes the resulting URLs; this service only
// handles event construction and relay publishing.
//
// Events are published concurrently to all `relayURLs`. Success requires at
// least one relay to acknowledge; individual failures are logged but not thrown.

struct NostrPodcastPublisher: Sendable {

    private static let logger = Logger.app("NostrPodcastPublisher")

    let publisher: any NostrEventPublishing
    let relayURLs: [URL]

    // MARK: - Publish show (kind:30074)

    /// Publish (or replace) the NIP-74 show event for an agent-owned podcast.
    /// Returns the signed Nostr event ID (32-byte hex).
    @discardableResult
    func publishShow(podcast: Podcast, signer: any NostrSigner) async throws -> String {
        var tags: [[String]] = [
            ["d", "podcast:guid:\(podcast.id.uuidString.lowercased())"],
            ["title", podcast.title],
        ]
        if !podcast.description.isEmpty {
            tags.append(["summary", podcast.description])
        }
        if !podcast.author.isEmpty {
            tags.append(["p", try await signer.publicKey()])
        }
        if let image = podcast.imageURL {
            tags.append(["image", image.absoluteString])
        }
        if let lang = podcast.language, !lang.isEmpty {
            tags.append(["language", lang])
        }
        for category in podcast.categories {
            tags.append(["t", category])
        }
        let draft = NostrEventDraft(kind: 30074, content: podcast.description, tags: tags)
        let signed = try await signer.sign(draft)
        try await publishToAll(event: signed)
        return signed.id
    }

    // MARK: - Publish episode (kind:30075)

    /// Publish (or replace) the NIP-74 episode event.
    /// Returns the signed Nostr event ID (32-byte hex).
    ///
    /// - Parameters:
    ///   - episode: The episode to publish.
    ///   - podcast: The owning podcast (must have an `ownerPubkeyHex`).
    ///   - audioURL: Blossom URL of the uploaded audio file.
    ///   - audioData: Raw audio bytes — used to compute the `x` (SHA-256) hash for `imeta`.
    ///   - chaptersURL: Optional Blossom URL of the uploaded chapters JSON.
    ///   - transcriptURL: Optional Blossom URL of the uploaded transcript.
    @discardableResult
    func publishEpisode(
        episode: Episode,
        podcast: Podcast,
        audioURL: URL,
        audioData: Data,
        chaptersURL: URL? = nil,
        transcriptURL: URL? = nil,
        signer: any NostrSigner
    ) async throws -> String {
        let pubkey = try await signer.publicKey()
        let showDTag = "podcast:guid:\(podcast.id.uuidString.lowercased())"
        let audioHash = Data(SHA256.hash(data: audioData)).hexString
        let pubDateSeconds = String(Int(episode.pubDate.timeIntervalSince1970))

        var tags: [[String]] = [
            ["d", "podcast:item:guid:\(episode.id.uuidString.lowercased())"],
            ["title", episode.title],
            ["published_at", pubDateSeconds],
            ["a", "30074:\(pubkey):\(showDTag)"],
        ]

        if !episode.description.isEmpty {
            tags.append(["summary", episode.description])
        }
        if let dur = episode.duration {
            tags.append(["duration", String(Int(dur))])
        }
        if let image = episode.imageURL ?? podcast.imageURL {
            tags.append(["image", image.absoluteString])
        }

        var imetaParts = [
            "url \(audioURL.absoluteString)",
            "m audio/mp4",
            "x \(audioHash)",
            "size \(audioData.count)",
        ]
        if let dur = episode.duration { imetaParts.append("duration \(Int(dur))") }
        tags.append(["imeta"] + imetaParts)

        if let chaptersURL {
            tags.append(["chapters", chaptersURL.absoluteString, "application/json+chapters"])
        }
        if let transcriptURL {
            tags.append(["transcript", transcriptURL.absoluteString, "text/vtt"])
        }

        let draft = NostrEventDraft(kind: 30075, content: episode.description, tags: tags)
        let signed = try await signer.sign(draft)
        try await publishToAll(event: signed)
        return signed.id
    }

    // MARK: - Multi-relay publish

    /// Publishes `event` to all configured relay URLs concurrently.
    /// Succeeds if at least one relay acknowledges; throws if all fail.
    private func publishToAll(event: SignedNostrEvent) async throws {
        guard !relayURLs.isEmpty else {
            throw NostrEventPublisherError.noRelayConfigured
        }
        var lastError: Error?
        var anySuccess = false

        await withTaskGroup(of: (URL, Error?).self) { group in
            for url in relayURLs {
                group.addTask {
                    do {
                        try await self.publisher.publish(event: event, relayURL: url)
                        return (url, nil)
                    } catch {
                        return (url, error)
                    }
                }
            }
            for await (url, error) in group {
                if let error {
                    Self.logger.warning("Relay \(url.host ?? url.absoluteString, privacy: .public) rejected event \(event.id.prefix(8), privacy: .public): \(error.localizedDescription, privacy: .public)")
                    lastError = error
                } else {
                    Self.logger.info("Published \(event.id.prefix(8), privacy: .public) → \(url.host ?? url.absoluteString, privacy: .public)")
                    anySuccess = true
                }
            }
        }

        if !anySuccess, let lastError {
            throw lastError
        }
    }
}
