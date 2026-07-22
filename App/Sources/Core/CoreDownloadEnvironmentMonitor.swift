import Foundation
import Network

enum DownloadNetworkStatus: Sendable, Equatable {
    case unknown
    case unavailable
    case wifi
    case other
}

/// Native-only capability observer. It reports facts; Rust owns admission,
/// prioritization, retry, and desired state.
@MainActor
final class CoreDownloadEnvironmentMonitor {
    static let shared = CoreDownloadEnvironmentMonitor()

    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(label: "io.f7z.podcast.core-download-environment")
    private var started = false
    private weak var client: SharedLibraryClient?

    func start(client: SharedLibraryClient) {
        self.client = client
        guard !started else { return }
        started = true
        monitor.pathUpdateHandler = { [weak self] path in
            let network: DownloadNetworkStatus = if path.status != .satisfied {
                .unavailable
            } else if path.usesInterfaceType(.wifi) {
                .wifi
            } else {
                .other
            }
            let capacity = Self.availableCapacity()
            Task { @MainActor [weak self] in
                self?.client?.observeDownloadEnvironment(
                    network: network,
                    availableCapacityBytes: capacity
                )
            }
        }
        monitor.start(queue: queue)
    }

    nonisolated private static func availableCapacity() -> Int64? {
        let root = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first
        return try? root?.resourceValues(
            forKeys: [.volumeAvailableCapacityForImportantUsageKey]
        ).volumeAvailableCapacityForImportantUsage
    }
}
