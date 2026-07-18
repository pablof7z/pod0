import Foundation
import Pod0Core

@MainActor
final class SharedLibraryClient {
    private struct Waiter {
        let continuation: CheckedContinuation<OperationResult?, Error>
    }

    private let facade: Pod0Facade
    private let dispatcher: Pod0NativeHostDispatcher
    private var subscriber: SharedLibrarySubscriber?
    private var subscriptionID: SubscriptionId?
    private var waiters: [CommandId: Waiter] = [:]
    private var lastRevision: UInt64 = 0
    private weak var store: AppStateStore?
    private var cachedSnapshot: SharedLibrarySnapshot?

    init(facade: Pod0Facade, feedHost: any CoreFeedHosting) {
        self.facade = facade
        self.dispatcher = Pod0NativeHostDispatcher(
            feedHost: feedHost,
            playbackHost: LibraryOnlyPlaybackHost()
        )
    }

    func start() {
        guard subscriptionID == nil else { return }
        let subscriber = SharedLibrarySubscriber { [weak self] projection in
            Task { @MainActor [weak self] in self?.receive(projection) }
        }
        self.subscriber = subscriber
        subscriptionID = facade.subscribe(
            request: ProjectionRequest(scope: .library, offset: 0, maxItems: 200),
            subscriber: subscriber
        )
    }

    func attach(store: AppStateStore) {
        self.store = store
        let snapshot = loadAllPages()
        cachedSnapshot = snapshot
        store.applySharedLibrary(snapshot)
    }

    func execute(_ command: ApplicationCommand) async throws -> OperationResult? {
        let commandID = CommandId(uuid: UUID())
        let cancellationID = CancellationId(uuid: UUID())
        return try await withCheckedThrowingContinuation { continuation in
            waiters[commandID] = Waiter(continuation: continuation)
            facade.dispatch(command: CommandEnvelope(
                commandId: commandID,
                cancellationId: cancellationID,
                expectedRevision: nil,
                command: command
            ))
            dispatcher.executePendingRequests(from: facade)
        }
    }

    func podcast(id: UUID) -> Podcast? {
        cachedSnapshot?.podcasts.first { $0.podcastId.uuid == id }?.swiftValue
    }

    func podcast(feedURL: URL) -> Podcast? {
        let key = feedURL.absoluteString.lowercased()
        return cachedSnapshot?.podcasts.first {
            $0.feedIdentity?.comparisonKey == key
        }?.swiftValue
    }

    func subscription(podcastID: UUID) -> PodcastSubscription? {
        cachedSnapshot?.subscriptions.first {
            $0.podcastId.uuid == podcastID
        }?.swiftValue
    }

    private func receive(_ envelope: ProjectionEnvelope) {
        guard envelope.stateRevision.value >= lastRevision else { return }
        lastRevision = envelope.stateRevision.value
        let snapshot = loadAllPages()
        cachedSnapshot = snapshot
        store?.applySharedLibrary(snapshot)
        resolveWaiters(snapshot.operations)
        dispatcher.executePendingRequests(from: facade)
    }

    private func loadAllPages() -> SharedLibrarySnapshot {
        var offset: UInt32 = 0
        var podcasts: [PodcastRecord] = []
        var subscriptions: [PodcastSubscriptionRecord] = []
        var episodes: [EpisodeRecord] = []
        var operations: [OperationProjection] = []
        while true {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .library,
                offset: offset,
                maxItems: 200
            ))
            guard case .library(let page) = envelope.projection else { break }
            podcasts.append(contentsOf: page.podcasts)
            subscriptions.append(contentsOf: page.subscriptions)
            episodes.append(contentsOf: page.episodes)
            if operations.isEmpty { operations = page.operations }
            guard page.hasMore else { break }
            guard offset <= UInt32.max - 200 else { break }
            offset += 200
        }
        return SharedLibrarySnapshot(
            podcasts: podcasts,
            subscriptions: subscriptions,
            episodes: episodes,
            operations: operations
        )
    }

    private func resolveWaiters(_ operations: [OperationProjection]) {
        for operation in operations {
            guard let waiter = waiters.removeValue(forKey: operation.commandId) else { continue }
            switch operation.stage {
            case .succeeded:
                waiter.continuation.resume(returning: operation.result)
            case .failed, .cancelled, .unsupported:
                waiter.continuation.resume(throwing: SharedLibraryError(operation.failure?.code))
            default:
                waiters[operation.commandId] = waiter
            }
        }
    }
}

struct SharedLibrarySnapshot {
    let podcasts: [PodcastRecord]
    let subscriptions: [PodcastSubscriptionRecord]
    let episodes: [EpisodeRecord]
    let operations: [OperationProjection]
}

enum SharedLibraryError: Error, LocalizedError, Equatable {
    case invalidURL
    case malformedFeed
    case alreadySubscribed
    case notFound
    case unavailable
    case cancelled

    init(_ code: CoreFailureCode?) {
        self = switch code {
        case .invalidFeedUrl: .invalidURL
        case .feedMalformed: .malformedFeed
        case .alreadySubscribed: .alreadySubscribed
        case .notFound: .notFound
        case .cancelled: .cancelled
        default: .unavailable
        }
    }

    var errorDescription: String? {
        switch self {
        case .invalidURL: "That doesn't look like a valid feed URL."
        case .malformedFeed: "Pod0 couldn't read a podcast feed at that address."
        case .alreadySubscribed: "You're already subscribed to this podcast."
        case .notFound: "That podcast is no longer in your library."
        case .unavailable: "Your library is temporarily unavailable."
        case .cancelled: "The library request was cancelled."
        }
    }
}

private final class SharedLibrarySubscriber: ProjectionSubscriber, @unchecked Sendable {
    private let delivery: @Sendable (ProjectionEnvelope) -> Void

    init(delivery: @escaping @Sendable (ProjectionEnvelope) -> Void) {
        self.delivery = delivery
    }

    func receive(projection: ProjectionEnvelope) {
        delivery(projection)
    }
}

@MainActor
private final class LibraryOnlyPlaybackHost: CorePlaybackHosting {
    func execute(_ request: HostRequest) -> HostObservation {
        .failed(code: .mediaUnavailable, safeDetail: "Playback host is not attached")
    }

    func installObservationSink(_ sink: @escaping (PlaybackLifecycleObservation) -> Void) {}
}
