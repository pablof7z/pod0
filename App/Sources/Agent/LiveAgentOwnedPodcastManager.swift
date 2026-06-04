import Foundation
import os.log

// MARK: - LiveAgentOwnedPodcastManager
//
// Production implementation of `AgentOwnedPodcastManagerProtocol`. After the
// owned-podcast lifecycle moved fully into the Rust kernel, this type is a thin
// wrapper: it routes create / update / delete through the kernel and lets the
// next kernel snapshot push reconcile the render store. The only real policy
// that remains Swift-side is the artwork generation pipeline (image-gen →
// Blossom upload) and the public-visibility-flip episode backfill (the kernel
// `Update` op carries no `visibility`, so the retro-publish of existing
// episodes on a private→public flip is sequenced here).
//
// Lifecycle ownership (Rust kernel):
//   podcast.create_podcast   — insert the feed-less row into the kernel store
//                              (the SSOT; `create_owned`/`publish_show` no-op
//                               without it).
//   create_owned_podcast     — generate + register the per-podcast keypair.
//   update_owned_podcast     — mutate metadata + re-publish kind:10154 when
//                              public + nostr-enabled (kernel owns the gate).
//   delete_owned_podcast     — NIP-09 deletion → drop key → remove row.
//   publish_show / publish_episode — sign + broadcast kind:10154 / kind:54.
//
// Dispatch is fire-and-forget — the signed event id / naddr lives in Rust's
// snapshot projection, not returned here.

final class LiveAgentOwnedPodcastManager: AgentOwnedPodcastManagerProtocol, @unchecked Sendable {

    private static let logger = Logger.app("AgentOwnedPodcastManager")

    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    // MARK: - Helpers

    @MainActor
    private func settings() -> Settings? { store?.state.settings }

    /// The active account's hex pubkey, sourced from the kernel (D13 — never a
    /// Swift-held private key). This value is only an OPTIMISTIC stamp for the
    /// new podcast row: the kernel generates a per-podcast keypair on
    /// `create_owned_podcast` and reconciles `owner_pubkey_hex` on the next
    /// snapshot tick (see `AppStateStore+KernelActions.kernelCreateOwnedPodcast`),
    /// so the field the kernel ultimately owns wins regardless.
    @MainActor
    private func agentPubkeyHex() throws -> String {
        guard let pubkey = store?.kernel?.kernelIdentity.activeAccount, !pubkey.isEmpty else {
            throw AgentOwnedPodcastError.noSigningKey
        }
        return pubkey
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
            pubkey = try await agentPubkeyHex()
        } else {
            pubkey = (try? await agentPubkeyHex()) ?? "agent-private"
        }
        // In-memory value only — the kernel store is the SSOT and projects the
        // row back on the next push. NOT written to `store.podcasts`.
        let stored = Podcast(
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
        await MainActor.run {
            // Insert the feed-less row into the Rust kernel store (SSOT).
            // Without this the key-registration + publish ops below no-op.
            store?.kernelCreatePodcast(
                podcastId: stored.id.uuidString,
                title: title,
                description: description,
                author: author,
                feedUrl: nil,
                artworkUrl: imageURL?.absoluteString,
                language: language,
                categories: categories,
                visibility: visibility.rawValue,
                titleIsPlaceholder: false
            )
        }
        Self.logger.info("Created agent-owned podcast '\(title, privacy: .public)' id=\(stored.id, privacy: .public)")
        // 3. Claim the per-podcast NIP-F4 signing key once, at creation,
        //    regardless of visibility — so a later private→public flip can
        //    publish without rotating the key. Publishing the show event is
        //    gated (public + nostrEnabled) inside the kernel.
        await MainActor.run { store?.kernelCreateOwnedPodcast(podcastId: stored.id.uuidString) }
        if visibility == .public, let settings = await settings(), settings.nostrEnabled {
            await MainActor.run { store?.kernelPublishShow(podcastId: stored.id.uuidString) }
        }
        return await MainActor.run { info(for: stored, nostrEventID: nil, nostrAddr: nil) }
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

        // Route the entire metadata update through the kernel. The kernel
        // mutates its own row (SSOT — including author + visibility, so the
        // next snapshot push doesn't revert this edit) and re-publishes the
        // kind:10154 show event when the podcast is public + nostr-enabled.
        // Swift no longer triggers a separate `publish_show`, and the
        // public/enabled gate is the kernel's. A private→public flip
        // republishes the show in the same op (the kernel applies the new
        // visibility before evaluating the gate).
        await MainActor.run {
            store?.kernelUpdateOwnedPodcast(
                podcastId: uuid.uuidString,
                title: title,
                description: description,
                author: author,
                artworkUrl: imageURL?.absoluteString,
                visibility: visibility?.rawValue
            )
            store?.updatePodcast(updated) // render mirror; snapshot push reconciles
        }

        var episodesPublished: Int?
        // Episode backfill on a private→public flip: the kernel republishes the
        // SHOW event itself, but per-episode kind:54 publishing is still
        // orchestrated Swift-side (the kernel update op has no episode-backfill
        // leg — tracked in BACKLOG owned-podcast-episode-backfill-kernel).
        if updated.nostrVisibility == .public, let settings = await settings(), settings.nostrEnabled {
            if wasPrivate {
                let episodes = await store?.episodes(forPodcast: uuid) ?? []
                for episode in episodes {
                    await MainActor.run { store?.kernelPublishEpisode(episodeId: episode.id.uuidString) }
                }
                episodesPublished = episodes.count
                Self.logger.info("Dispatched NIP-F4 publish for \(episodes.count) episodes of '\(updated.title, privacy: .public)'")
            }
        }
        return await MainActor.run {
            info(for: updated, nostrEventID: nil, nostrAddr: nil, episodesPublishedToNostr: episodesPublished)
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
        // Full kernel-owned deletion: publish NIP-09 (kind:5) for the prior
        // show event, drop the per-podcast key, remove the row + episodes from
        // the kernel store. Replaces the old `store.deletePodcast` →
        // `kernelUnsubscribe` path, which removed the row but leaked the key
        // and never published a deletion.
        await MainActor.run {
            guard let store else { return }
            store.kernelDeleteOwnedPodcast(podcastId: uuid.uuidString)
            // Render mirror — drop the local row immediately; the next snapshot
            // push reconciles. `deletePodcast` also cleans subscriptions /
            // episodes / wiki citations Swift-side.
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
        // Auth signing is the kernel's job (D13 sign-for-return): the
        // KernelSigner signs the kind:24242 auth event with the active account —
        // no private key in Swift. The artwork blob isn't owned by the
        // per-podcast key; any valid auth signature the Blossom server accepts
        // suffices (it only checks the `x` hash tag matches the upload).
        guard let kernel = await store?.kernel else {
            throw AgentOwnedPodcastError.storeUnavailable
        }
        let blossom = BlossomUploader(serverURLString: settings.blossomServerURL)
        let url = try await blossom.upload(
            data: imageData,
            contentType: "image/png",
            signer: KernelSigner(kernel: kernel))
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

        await dispatchPublishEpisode(episodeID: uuid)
        Self.logger.info("Dispatched NIP-F4 publish for episode '\(episode.title, privacy: .public)'")
        // Dispatch is fire-and-forget; the signed event id / naddr now lives in
        // Rust's snapshot projection. Return a non-nil status marker so callers
        // (publish_episode tool) report success rather than a false "skipped".
        return "nipf4:publish_dispatched"
    }

    // MARK: - Private NIP-F4 dispatch helpers

    /// Publish the `kind:54` episode event. Rust resolves the parent podcast +
    /// its per-podcast key and uploads audio to Blossom. Show creation /
    /// claim / update / delete dispatch their kernel ops inline at their call
    /// sites (this remains a helper only because `publishEpisodeToNostr` gates
    /// the dispatch behind owner/visibility/nostrEnabled checks).
    private func dispatchPublishEpisode(episodeID: UUID) async {
        let id = episodeID.uuidString
        await MainActor.run {
            store?.kernelPublishEpisode(episodeId: id)
        }
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
}

enum AgentOwnedPodcastError: LocalizedError {
    case storeUnavailable
    case noSigningKey
    case invalidID(String)
    case notFound(String)
    case notOwned(String)
    case episodeNotFound(String)

    var errorDescription: String? {
        switch self {
        case .storeUnavailable: return "App state is unavailable."
        case .noSigningKey: return "No Nostr signing key configured. Set up your identity in Settings > Agent > Identity."
        case .invalidID(let id): return "Invalid UUID: \(id)"
        case .notFound(let id): return "Podcast not found: \(id)"
        case .notOwned(let id): return "Podcast \(id) is not agent-owned."
        case .episodeNotFound(let id): return "Episode not found: \(id)"
        }
    }
}
