import Foundation
import os.log

// MARK: - LiveAgentOwnedPodcastManager
//
// Production implementation of `AgentOwnedPodcastManagerProtocol`. After the
// owned-podcast lifecycle moved fully into the Rust kernel, this type is a thin
// wrapper: it routes create / update / delete / publish through the kernel and lets the
// next kernel snapshot push reconcile the render store. The only real policy
// that remains Swift-side is the artwork generation pipeline (image-gen →
// Blossom upload). Per-episode kind:54 backfill on a private→public flip is
// now owned by the kernel (D0 — update_owned detects the flip and self-enqueues
// one publish_episode op per episode, so the actor yields between uploads
// instead of blocking — D8).
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

    private struct CreateLifecyclePlan: Decodable {
        let error: String?
        let ownerPubkeyHex: String?
        let shouldPublishShow: Bool

        enum CodingKeys: String, CodingKey {
            case error
            case ownerPubkeyHex = "owner_pubkey_hex"
            case shouldPublishShow = "should_publish_show"
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            error = try c.decodeIfPresent(String.self, forKey: .error)
            ownerPubkeyHex = try c.decodeIfPresent(String.self, forKey: .ownerPubkeyHex)
            shouldPublishShow = try c.decodeIfPresent(Bool.self, forKey: .shouldPublishShow) ?? false
        }
    }

    private struct MutationPreflight: Decodable {
        let error: String?
        let ok: Bool?
    }

    private static let logger = Logger.app("AgentOwnedPodcastManager")

    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    // MARK: - Helpers

    @MainActor
    private func settings() -> Settings? { store?.state.settings }

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
        let plan = try await createLifecyclePlan(visibility: visibility)
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
            ownerPubkeyHex: plan.ownerPubkeyHex,
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
        if plan.shouldPublishShow {
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
        let uuid = UUID(uuidString: podcastID)
        let existing: Podcast?
        if let uuid {
            existing = await MainActor.run { store?.podcast(id: uuid) }
        } else {
            existing = nil
        }
        try await mutationPreflight(podcastID: podcastID, existing: existing)
        guard let uuid, let existing else {
            throw AgentOwnedPodcastError.policy("Podcast update preflight failed.")
        }
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
        }

        // Episode backfill on a private→public flip is now owned by the kernel:
        // update_owned detects the flip and self-enqueues one publish_episode op
        // per episode (D0 — Rust owns publish policy; D8 — the actor yields
        // between uploads instead of blocking on a synchronous loop).
        return await MainActor.run {
            info(for: updated, nostrEventID: nil, nostrAddr: nil)
        }
    }

    // MARK: - deletePodcast

    func deletePodcast(podcastID: PodcastID) async throws {
        let uuid = UUID(uuidString: podcastID)
        let existing: Podcast?
        if let uuid {
            existing = await MainActor.run { store?.podcast(id: uuid) }
        } else {
            existing = nil
        }
        try await mutationPreflight(podcastID: podcastID, existing: existing)
        guard let uuid else {
            throw AgentOwnedPodcastError.policy("Podcast delete preflight failed.")
        }
        // Full kernel-owned deletion: publish NIP-09 (kind:5) for the prior
        // show event, drop the per-podcast key, remove the row + episodes from
        // the kernel store. Replaces the old `store.deletePodcast` →
        // `kernelUnsubscribe` path, which removed the row but leaked the key
        // and never published a deletion.
        await MainActor.run {
            guard let store else { return }
            store.kernelDeleteOwnedPodcast(podcastId: uuid.uuidString)
        }
    }

    // MARK: - listOwnedPodcasts

    func listOwnedPodcasts() async -> [AgentOwnedPodcastInfo] {
        guard let store else { return [] }
        let podcasts = await MainActor.run { store.rustOwnedPodcasts() }
        return await MainActor.run { podcasts.map { info(for: $0, nostrEventID: nil, nostrAddr: nil) } }
    }

    // MARK: - generateAndUploadArtwork

    func generateAndUploadArtwork(prompt: String) async throws -> URL {
        guard let settings = await settings() else {
            throw AgentOwnedPodcastError.storeUnavailable
        }
        let imageGen = ImageGenerationService()
        let imageData = try await imageGen.generate(prompt: prompt, model: settings.imageGenerationModel)
        // D13/D0: kernel owns the kind:24242 auth signing AND the HTTP transport
        // (nmp.blossom.upload). No private key in Swift; no URLSession in Swift.
        // Active account signs (artwork blob — no per-podcast key needed).
        guard let kernel = await store?.kernel else {
            throw AgentOwnedPodcastError.storeUnavailable
        }
        let url = try await kernel.blossomUpload(
            data: imageData,
            contentType: "image/png",
            servers: [settings.blossomServerURL])
        Self.logger.info("Artwork uploaded to \(url.absoluteString, privacy: .public)")
        return url
    }

    private func createLifecyclePlan(visibility: Podcast.NostrVisibility) async throws -> CreateLifecyclePlan {
        let activePubkey = await MainActor.run {
            store?.kernel?.kernelIdentity.activeAccount
        }
        let nostrEnabled = await MainActor.run { store?.state.settings.nostrEnabled ?? false }
        var payload: [String: Any] = [
            "op": "create_lifecycle_plan",
            "visibility": visibility.rawValue,
            "nostr_enabled": nostrEnabled,
        ]
        if let activePubkey, !activePubkey.isEmpty {
            payload["active_pubkey"] = activePubkey
        }
        let plan = try await ownedPodcastPolicy(CreateLifecyclePlan.self, payload: payload)
        if let error = plan.error {
            throw AgentOwnedPodcastError.policy(error)
        }
        return plan
    }

    private func mutationPreflight(podcastID: PodcastID, existing: Podcast?) async throws {
        let response = try await ownedPodcastPolicy(
            MutationPreflight.self,
            payload: [
                "op": "mutation_preflight",
                "podcast_id": podcastID,
                "exists": existing != nil,
                "is_owned": existing?.ownerPubkeyHex != nil,
            ]
        )
        if let error = response.error {
            throw AgentOwnedPodcastError.policy(error)
        }
        guard response.ok == true else {
            throw AgentOwnedPodcastError.policy("Owned-podcast mutation was rejected.")
        }
    }

    private func ownedPodcastPolicy<T: Decodable>(_ type: T.Type, payload: [String: Any]) async throws -> T {
        let handleBits = await MainActor.run {
            store?.kernel?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }
        guard let handleBits else { throw AgentOwnedPodcastError.storeUnavailable }
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { throw AgentOwnedPodcastError.storeUnavailable }
        let envelope = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"App state is unavailable."}"#
            }
            return json.withCString { ptr -> String in
                guard let result = nmp_app_podcast_agent_owned_podcast_tool(handle, ptr) else {
                    return #"{"error":"App state is unavailable."}"#
                }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
        guard let responseData = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder().decode(T.self, from: responseData)
        else { throw AgentOwnedPodcastError.storeUnavailable }
        return decoded
    }

    // MARK: - publishEpisodeToNostr

    func publishEpisodeToNostr(episodeID: EpisodeID) async throws -> String? {
        guard let store else { throw AgentOwnedPodcastError.storeUnavailable }
        await MainActor.run {
            store.kernelPublishEpisode(episodeId: episodeID)
        }
        Self.logger.info("Dispatched NIP-F4 publish for episode id=\(episodeID, privacy: .public)")
        // Dispatch is fire-and-forget; the signed event id / naddr now lives in
        // Rust's snapshot projection. Return a non-nil status marker so callers
        // (publish_episode tool) report success rather than a false "skipped".
        return "nipf4:publish_dispatched"
    }

    @MainActor
    private func info(
        for podcast: Podcast,
        nostrEventID: String?,
        nostrAddr: String?,
        episodesPublishedToNostr: Int? = nil
    ) -> AgentOwnedPodcastInfo {
        let episodeCount = store?.rustEpisodeCount(forPodcast: podcast.id) ?? 0
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
    case policy(String)

    var errorDescription: String? {
        switch self {
        case .storeUnavailable: return "App state is unavailable."
        case .noSigningKey: return "No Nostr signing key configured. Set up your identity in Settings > Agent > Identity."
        case .invalidID(let id): return "Invalid UUID: \(id)"
        case .notFound(let id): return "Podcast not found: \(id)"
        case .notOwned(let id): return "Podcast \(id) is not agent-owned."
        case .episodeNotFound(let id): return "Episode not found: \(id)"
        case .policy(let message): return message
        }
    }
}
