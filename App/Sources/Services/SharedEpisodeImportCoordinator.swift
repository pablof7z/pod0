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

    @ObservationIgnored private var isConsuming = false
    @ObservationIgnored private var dismissalTask: Task<Void, Never>?

    func consumePending(
        store: AppStateStore,
        onImported: @escaping @MainActor (UUID) -> Void
    ) async {
        guard let requestStore = try? SharedEpisodeImportRequestStore.appGroup() else { return }
        await consumePending(from: requestStore, store: store, onImported: onImported)
    }

    /// Real consume logic, separated from `consumePending(store:onImported:)`
    /// so tests can inject an isolated `SharedEpisodeImportRequestStore`
    /// instead of the real App Group container.
    func consumePending(
        from requestStore: SharedEpisodeImportRequestStore,
        store: AppStateStore,
        onImported: @escaping @MainActor (UUID) -> Void
    ) async {
        guard !isConsuming else { return }
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
                    requestID: request.id,
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
        requestID: UUID,
        store: AppStateStore
    ) async throws -> Episode {
        guard let sharedLibrary = store.sharedLibrary else {
            throw SharedLibraryError.unavailable
        }
        let episodeID = try await sharedLibrary.importSharedEpisode(
            sourceURL: sourceURL,
            requestID: requestID
        )
        guard let episode = store.episode(id: episodeID) else {
            throw SharedLibraryError.unavailable
        }
        return episode
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

}
