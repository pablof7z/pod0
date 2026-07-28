import Foundation
import Observation
import Pod0Core

@MainActor
@Observable
final class SharedEpisodeImportCoordinator {
    enum Phase: Equatable {
        case importing
        case downloadStarted(title: String)
        case failed(message: String)
    }

    private(set) var phase: Phase?

    @ObservationIgnored private let resolver: SharedEpisodeResolver
    @ObservationIgnored private var isConsuming = false
    @ObservationIgnored private var dismissalTask: Task<Void, Never>?

    init(resolver: SharedEpisodeResolver = SharedEpisodeResolver()) {
        self.resolver = resolver
    }

    func consumePending(
        store: AppStateStore,
        onImported: @escaping @MainActor (UUID) -> Void
    ) async {
        guard !isConsuming else { return }
        let requestStore: SharedEpisodeImportRequestStore
        do {
            requestStore = try .appGroup()
        } catch {
            return
        }

        let requests: [SharedEpisodeImportRequest]
        do {
            requests = try requestStore.pendingRequests()
        } catch {
            presentFailure(error)
            return
        }
        guard !requests.isEmpty else { return }

        isConsuming = true
        dismissalTask?.cancel()
        defer { isConsuming = false }

        for request in requests {
            phase = .importing
            do {
                let episode = try await importEpisode(
                    from: request.sourceURL,
                    store: store
                )
                try requestStore.remove(request)
                store.sharedLibrary?.requestDownload(
                    episodeID: episode.id,
                    origin: .user
                )
                phase = .downloadStarted(title: episode.title)
                onImported(episode.id)
            } catch {
                try? requestStore.remove(request)
                presentFailure(error)
                break
            }
        }
        scheduleSuccessDismissal()
    }

    func dismissFailure() {
        if case .failed = phase { phase = nil }
    }

    private func importEpisode(
        from sourceURL: URL,
        store: AppStateStore
    ) async throws -> Episode {
        let resolved = try await resolver.resolve(sourceURL)
        let podcastID: UUID
        if let feedURL = resolved.feedURL {
            podcastID = store.state.podcasts.first(where: {
                Self.comparableFeedURL($0.feedURL) == Self.comparableFeedURL(feedURL)
            })?.id ?? Self.stablePodcastID(for: feedURL.absoluteString)
        } else {
            podcastID = Self.stablePodcastID(
                for: "synthetic:\(resolved.podcastTitle.lowercased())"
            )
            if store.podcast(id: podcastID) == nil {
                _ = try await store.upsertSyntheticPodcastAndWait(
                    Podcast(
                        id: podcastID,
                        kind: .synthetic,
                        title: resolved.podcastTitle,
                        imageURL: resolved.imageURL,
                        description: "Episode shared into Pod0"
                    )
                )
            }
        }

        let episode = try await store.upsertExternalEpisodeAndWait(
            podcastID: podcastID,
            feedURL: resolved.feedURL,
            podcastTitle: resolved.podcastTitle,
            audioURL: resolved.audioURL,
            title: resolved.title,
            description: resolved.description,
            publishedAt: resolved.publishedAt,
            enclosureMimeType: resolved.enclosureMIMEType,
            imageURL: resolved.imageURL,
            duration: resolved.duration
        )
        if resolved.feedURL != nil {
            _ = try? await store.sharedLibrary?.execute(.hydratePodcastMetadata(
                podcastId: PodcastId(uuid: episode.podcastID)
            ))
        }
        return store.episode(id: episode.id) ?? episode
    }

    private func presentFailure(_ error: Error) {
        dismissalTask?.cancel()
        phase = .failed(
            message: (error as? LocalizedError)?.errorDescription
                ?? "Pod0 could not add that episode."
        )
    }

    private func scheduleSuccessDismissal() {
        guard case .downloadStarted = phase else { return }
        dismissalTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(3))
            guard !Task.isCancelled else { return }
            if case .downloadStarted = self?.phase {
                self?.phase = nil
            }
        }
    }

    private static func comparableFeedURL(_ url: URL?) -> String? {
        guard let url else { return nil }
        return url.absoluteString
            .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            .lowercased()
    }

    private static func stablePodcastID(for identity: String) -> UUID {
        let bytes = Array(identity.utf8)
        var first: UInt64 = 14_695_981_039_346_656_037
        var second: UInt64 = 7_809_847_782_465_536_322
        for byte in bytes {
            first = (first ^ UInt64(byte)) &* 1_099_511_628_211
            second = (second ^ UInt64(byte &+ 31)) &* 1_099_511_628_211
        }
        var value = withUnsafeBytes(of: first.bigEndian, Array.init)
        value.append(contentsOf: withUnsafeBytes(of: second.bigEndian, Array.init))
        value[6] = (value[6] & 0x0F) | 0x50
        value[8] = (value[8] & 0x3F) | 0x80
        return UUID(uuid: (
            value[0], value[1], value[2], value[3],
            value[4], value[5], value[6], value[7],
            value[8], value[9], value[10], value[11],
            value[12], value[13], value[14], value[15]
        ))
    }
}
