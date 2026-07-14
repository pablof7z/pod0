import CryptoKit
import Foundation
import os.log

// MARK: - LiveAgentOwnedPodcastManager
//
// Production implementation of `AgentOwnedPodcastManagerProtocol`. Owns the
// full lifecycle: store mutations, image generation, Blossom uploads, and
// NIP-74 event publishing. Constructed once per `AgentChatSession` via
// `LivePodcastAgentToolDeps.make(...)`.

final class LiveAgentOwnedPodcastManager: AgentOwnedPodcastManagerProtocol, @unchecked Sendable {

    private static let logger = Logger.app("AgentOwnedPodcastManager")

    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    // MARK: - Helpers

    @MainActor
    private func settings() -> Settings? { store?.state.settings }

    nonisolated private func nostrSigner() throws -> LocalKeySigner {
        guard let hex = try NostrCredentialStore.privateKey(), !hex.isEmpty else {
            throw AgentOwnedPodcastError.noSigningKey
        }
        let kp = try NostrKeyPair(privateKeyHex: hex)
        return LocalKeySigner(keyPair: kp)
    }

    nonisolated private func agentPubkeyHex() throws -> String {
        try nostrSigner().keyPair.publicKeyHex
    }

    // MARK: - createPodcast

    func createPodcast(
        title: String,
        description: String,
        author: String,
        imageURL: URL?,
        language: String?,
        categories: [String],
        visibility: Podcast.NostrVisibility
    ) async throws -> AgentOwnedPodcastInfo {
        let pubkey: String
        if visibility == .public {
            pubkey = try agentPubkeyHex()
        } else {
            pubkey = (try? agentPubkeyHex()) ?? "agent-private"
        }
        let podcast = Podcast(
            kind: .synthetic,
            feedURL: nil,
            title: title,
            author: author,
            imageURL: imageURL,
            description: description,
            language: language,
            categories: categories,
            ownerPubkeyHex: pubkey,
            nostrVisibility: visibility
        )
        let stored = await MainActor.run {
            store?.upsertPodcast(podcast) ?? podcast
        }
        Self.logger.info("Created agent-owned podcast '\(title, privacy: .public)' id=\(stored.id, privacy: .public)")
        var showEventID: String?
        if visibility == .public, let settings = await settings(), settings.nostrEnabled {
            showEventID = try? await publishShowEvent(podcast: stored, settings: settings)
        }
        let naddr = nostrAddr(for: stored, eventID: showEventID)
        return await MainActor.run { info(for: stored, nostrEventID: showEventID, nostrAddr: naddr) }
    }

    // MARK: - updatePodcast

    func updatePodcast(
        podcastID: PodcastID,
        title: String?,
        description: String?,
        author: String?,
        imageURL: URL?,
        visibility: Podcast.NostrVisibility?
    ) async throws -> AgentOwnedPodcastInfo {
        guard let uuid = UUID(uuidString: podcastID) else {
            throw AgentOwnedPodcastError.invalidID(podcastID)
        }
        guard let existing = await store?.podcast(id: uuid) else {
            throw AgentOwnedPodcastError.notFound(podcastID)
        }
        guard existing.ownerPubkeyHex != nil else {
            throw AgentOwnedPodcastError.notOwned(podcastID)
        }
        let wasPrivate = existing.nostrVisibility != .public
        var updated = existing
        if let title { updated.title = title }
        if let description { updated.description = description }
        if let author { updated.author = author }
        if let imageURL { updated.imageURL = imageURL }
        if let visibility { updated.nostrVisibility = visibility }
        await MainActor.run { store?.updatePodcast(updated) }

        var showEventID: String?
        var episodesPublished: Int?
        if updated.nostrVisibility == .public, let settings = await settings(), settings.nostrEnabled {
            showEventID = try? await publishShowEvent(podcast: updated, settings: settings)
            // Retroactively publish all existing episodes when flipping to public.
            if wasPrivate {
                let episodes = await store?.episodes(forPodcast: uuid) ?? []
                var published = 0
                for episode in episodes {
                    do {
                        try await publishEpisodeRecord(episode, podcast: updated, settings: settings)
                        published += 1
                    } catch {
                        Self.logger.warning("Failed to publish episode '\(episode.title, privacy: .public)': \(error.localizedDescription, privacy: .public)")
                    }
                }
                episodesPublished = published
                Self.logger.info("Batch-published \(published)/\(episodes.count) episodes for '\(updated.title, privacy: .public)'")
            }
        }
        let naddr = nostrAddr(for: updated, eventID: showEventID)
        return await MainActor.run {
            info(for: updated, nostrEventID: showEventID, nostrAddr: naddr, episodesPublishedToNostr: episodesPublished)
        }
    }

    // MARK: - deletePodcast

    func deletePodcast(podcastID: PodcastID) async throws {
        guard let uuid = UUID(uuidString: podcastID) else {
            throw AgentOwnedPodcastError.invalidID(podcastID)
        }
        guard let existing = await store?.podcast(id: uuid) else {
            throw AgentOwnedPodcastError.notFound(podcastID)
        }
        guard existing.ownerPubkeyHex != nil else {
            throw AgentOwnedPodcastError.notOwned(podcastID)
        }
        await MainActor.run {
            guard let store else { return }
            store.deletePodcast(podcastID: uuid)
        }
    }

    // MARK: - listOwnedPodcasts

    func listOwnedPodcasts() async -> [AgentOwnedPodcastInfo] {
        guard let store else { return [] }
        let podcasts = await store.allPodcasts.filter { $0.ownerPubkeyHex != nil }
        return await MainActor.run { podcasts.map { info(for: $0, nostrEventID: nil, nostrAddr: nil) } }
    }

    // MARK: - generateAndUploadArtwork

    func generateAndUploadArtwork(prompt: String) async throws -> URL {
        guard let settings = await settings() else {
            throw AgentOwnedPodcastError.storeUnavailable
        }
        guard settings.openRouterCredentialSource != .none,
              let apiKey = try? OpenRouterCredentialStore.apiKey(),
              !apiKey.isEmpty else {
            throw ImageGenerationError.noAPIKey
        }
        let imageGen = ImageGenerationService(apiKey: apiKey)
        let imageData = try await imageGen.generate(prompt: prompt, model: settings.imageGenerationModel)
        let signer = try nostrSigner()
        let blossom = BlossomUploader(serverURLString: settings.blossomServerURL)
        let url = try await blossom.upload(data: imageData, contentType: "image/png", signer: signer)
        Self.logger.info("Artwork uploaded to \(url.absoluteString, privacy: .public)")
        return url
    }

    // MARK: - publishEpisodeToNostr

    func publishEpisodeToNostr(episodeID: EpisodeID) async throws -> String? {
        guard let uuid = UUID(uuidString: episodeID),
              let store else { throw AgentOwnedPodcastError.storeUnavailable }
        guard let episode = await store.episode(id: uuid) else {
            throw AgentOwnedPodcastError.episodeNotFound(episodeID)
        }
        guard let podcast = await store.podcast(id: episode.podcastID),
              podcast.ownerPubkeyHex != nil,
              podcast.nostrVisibility == .public else { return nil }
        guard let settings = await settings(), settings.nostrEnabled else { return nil }

        let eventID = try await publishEpisodeRecord(episode, podcast: podcast, settings: settings)
        let dTag = "podcast:item:guid:\(episode.id.uuidString.lowercased())"
        guard let pubkeyHex = podcast.ownerPubkeyHex else { return eventID }
        let relays = effectivePublicRelays(settings: settings)
        let relayHint = relays.first ?? settings.nostrRelayURL
        let naddr = NIP19.naddr(dTag: dTag, pubkeyHex: pubkeyHex, kind: 30075, relayURL: relayHint)
        Self.logger.info("Published episode '\(episode.title, privacy: .public)' to Nostr NIP-74")
        return naddr ?? eventID
    }

    // MARK: - Private helpers

    /// Resolves the effective public relay list: stored list if non-empty, else defaults.
    nonisolated private func effectivePublicRelays(settings: Settings) -> [String] {
        let stored = settings.nostrPublicRelays.filter { !$0.isEmpty }
        return stored.isEmpty ? NIP65RelayFetcher.defaultRelays : stored
    }

    /// Publishes the NIP-74 show kind:30074 event and returns the event ID.
    @discardableResult
    nonisolated private func publishShowEvent(podcast: Podcast, settings: Settings) async throws -> String {
        let relayURLs = effectivePublicRelays(settings: settings).compactMap { URL(string: $0) }
        guard !relayURLs.isEmpty else {
            throw AgentOwnedPodcastError.noRelayConfigured
        }
        let signer = try nostrSigner()
        let publisher = NostrPodcastPublisher(
            publisher: NostrWebSocketEventPublisher(),
            relayURLs: relayURLs
        )
        return try await publisher.publishShow(podcast: podcast, signer: signer)
    }

    /// Uploads audio/chapters/transcript and publishes kind:30075. Returns event ID.
    @discardableResult
    nonisolated private func publishEpisodeRecord(
        _ episode: Episode,
        podcast: Podcast,
        settings: Settings
    ) async throws -> String {
        let relayURLs = effectivePublicRelays(settings: settings).compactMap { URL(string: $0) }
        guard !relayURLs.isEmpty else {
            throw AgentOwnedPodcastError.noRelayConfigured
        }
        let signer = try nostrSigner()
        let blossom = BlossomUploader(serverURLString: settings.blossomServerURL)
        let publisher = NostrPodcastPublisher(
            publisher: NostrWebSocketEventPublisher(),
            relayURLs: relayURLs
        )

        // Upload audio
        let audioData: Data
        if case .downloaded(let localURL, _) = episode.downloadState {
            do {
                audioData = try Data(contentsOf: localURL)
            } catch {
                Self.logger.error("publishEpisodeRecord: cannot read audio for '\(episode.title, privacy: .public)' at \(localURL.lastPathComponent, privacy: .public): \(error.localizedDescription, privacy: .public)")
                audioData = Data()
            }
        } else {
            audioData = Data()
        }
        let audioBlossomURL: URL
        if !audioData.isEmpty {
            audioBlossomURL = try await blossom.upload(data: audioData, contentType: "audio/mp4", signer: signer)
        } else {
            // Only use the enclosure URL directly when it is a remote (HTTP/HTTPS) URL —
            // for example, an RSS episode not yet downloaded. A local file:// URL must
            // never appear in a published Nostr event.
            let scheme = episode.enclosureURL.scheme?.lowercased() ?? ""
            guard scheme == "http" || scheme == "https" else {
                throw AgentOwnedPodcastError.audioNotAvailable(episode.title)
            }
            audioBlossomURL = episode.enclosureURL
        }

        // Upload chapters JSON if available
        var chaptersBlossomURL: URL?
        if let chaptersData = serializeChapters(episode.chapters) {
            chaptersBlossomURL = try? await blossom.upload(data: chaptersData, contentType: "application/json", signer: signer)
        } else if let chaptersURL = episode.chaptersURL,
                  let chaptersData = try? Data(contentsOf: chaptersURL) {
            chaptersBlossomURL = try? await blossom.upload(data: chaptersData, contentType: "application/json", signer: signer)
        }

        // Upload transcript if available
        var transcriptBlossomURL: URL?
        if case .ready = episode.transcriptState,
           let transcriptData = loadTranscriptData(episodeID: episode.id) {
            transcriptBlossomURL = try? await blossom.upload(data: transcriptData, contentType: "text/vtt", signer: signer)
        }

        return try await publisher.publishEpisode(
            episode: episode,
            podcast: podcast,
            audioURL: audioBlossomURL,
            audioData: audioData.isEmpty ? Data(episode.enclosureURL.absoluteString.utf8) : audioData,
            chaptersURL: chaptersBlossomURL,
            transcriptURL: transcriptBlossomURL,
            signer: signer
        )
    }

    /// Builds the naddr for a podcast show event.
    nonisolated private func nostrAddr(for podcast: Podcast, eventID: String?) -> String? {
        guard let pubkeyHex = podcast.ownerPubkeyHex else { return nil }
        let dTag = "podcast:guid:\(podcast.id.uuidString.lowercased())"
        return NIP19.naddr(dTag: dTag, pubkeyHex: pubkeyHex, kind: 30074)
    }

    @MainActor
    private func info(
        for podcast: Podcast,
        nostrEventID: String?,
        nostrAddr: String?,
        episodesPublishedToNostr: Int? = nil
    ) -> AgentOwnedPodcastInfo {
        let episodeCount = (store?.episodes(forPodcast: podcast.id) ?? []).count
        return AgentOwnedPodcastInfo(
            podcastID: podcast.id.uuidString,
            title: podcast.title,
            description: podcast.description,
            author: podcast.author,
            imageURL: podcast.imageURL,
            visibility: podcast.nostrVisibility.rawValue,
            episodeCount: episodeCount,
            nostrEventID: nostrEventID,
            nostrAddr: nostrAddr,
            episodesPublishedToNostr: episodesPublishedToNostr
        )
    }

    nonisolated private func loadTranscriptData(episodeID: UUID) -> Data? {
        guard let transcript = TranscriptStore.shared.load(episodeID: episodeID) else { return nil }
        var vtt = "WEBVTT\n\n"
        for (i, seg) in transcript.segments.enumerated() {
            vtt += "\(i + 1)\n"
            vtt += "\(formatVTTTime(seg.start)) --> \(formatVTTTime(seg.end))\n"
            vtt += "\(seg.text)\n\n"
        }
        return vtt.data(using: .utf8)
    }

    nonisolated private func serializeChapters(_ chapters: [Episode.Chapter]?) -> Data? {
        guard let chapters, !chapters.isEmpty else { return nil }
        let rows = chapters.map { ch -> [String: Any] in
            var row: [String: Any] = ["startTime": ch.startTime, "title": ch.title]
            if let url = ch.linkURL { row["url"] = url.absoluteString }
            if let img = ch.imageURL { row["img"] = img.absoluteString }
            return row
        }
        let json: [String: Any] = ["version": "1.2.0", "chapters": rows]
        return try? JSONSerialization.data(withJSONObject: json, options: [.prettyPrinted])
    }

    nonisolated private func formatVTTTime(_ seconds: Double) -> String {
        let h = Int(seconds) / 3600
        let m = (Int(seconds) % 3600) / 60
        let s = Int(seconds) % 60
        let ms = Int((seconds - Double(Int(seconds))) * 1000)
        return String(format: "%02d:%02d:%02d.%03d", h, m, s, ms)
    }
}

enum AgentOwnedPodcastError: LocalizedError {
    case storeUnavailable
    case noSigningKey
    case noRelayConfigured
    case invalidID(String)
    case notFound(String)
    case notOwned(String)
    case episodeNotFound(String)
    case audioNotAvailable(String)

    var errorDescription: String? {
        switch self {
        case .storeUnavailable: return "App state is unavailable."
        case .noSigningKey: return "No Nostr signing key configured. Set up your identity in Settings > Agent > Identity."
        case .noRelayConfigured: return "No Nostr relay configured. Set a relay URL in Settings > Agent > Nostr."
        case .invalidID(let id): return "Invalid UUID: \(id)"
        case .notFound(let id): return "Podcast not found: \(id)"
        case .notOwned(let id): return "Podcast \(id) is not agent-owned."
        case .episodeNotFound(let id): return "Episode not found: \(id)"
        case .audioNotAvailable(let title): return "Audio file for '\(title)' could not be read for upload."
        }
    }
}
