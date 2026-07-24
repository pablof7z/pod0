import Foundation
import Pod0Core
import UserNotifications

enum CoreNotificationAuthorization: Equatable {
    case notDetermined
    case denied
    case authorized
    case unsupported
}

struct CoreNotificationRequest: Equatable {
    let identifier: String
    let title: String
    let body: String
    let threadIdentifier: String
    let episodeID: String
    let occurrenceID: String
}

@MainActor
protocol CoreNotificationCentering: AnyObject {
    func authorization() async -> CoreNotificationAuthorization
    func requestAuthorization() async throws -> Bool
    func existingRequestIdentifiers() async -> Set<String>
    func add(_ request: CoreNotificationRequest) async throws
    func remove(identifier: String)
}

@MainActor
protocol CoreNotificationHosting: AnyObject {
    func deliver(
        occurrenceID: FeedDiscoveryOccurrenceId,
        episodeID: EpisodeId,
        podcastID: PodcastId,
        podcastTitle: String,
        episodeTitle: String
    ) async -> HostObservation
    func cancel(occurrenceID: FeedDiscoveryOccurrenceId)
    func shutdown()
}

@MainActor
final class CoreNotificationHost: CoreNotificationHosting {
    private let center: any CoreNotificationCentering
    private var deliveries: [String: Task<HostObservation, Never>] = [:]

    init(center: any CoreNotificationCentering = SystemCoreNotificationCenter()) {
        self.center = center
    }

    func deliver(
        occurrenceID: FeedDiscoveryOccurrenceId,
        episodeID: EpisodeId,
        podcastID: PodcastId,
        podcastTitle: String,
        episodeTitle: String
    ) async -> HostObservation {
        let identifier = occurrenceID.stableString
        if let delivery = deliveries[identifier] {
            return await delivery.value
        }
        guard let episodeUUID = episodeID.uuid, let podcastUUID = podcastID.uuid else {
            return .failed(
                code: .invalidResponse,
                safeDetail: "Notification identifiers are invalid"
            )
        }
        let request = CoreNotificationRequest(
            identifier: identifier,
            title: podcastTitle,
            body: "New episode: \(episodeTitle)",
            threadIdentifier: "podcast:\(podcastUUID.uuidString)",
            episodeID: episodeUUID.uuidString,
            occurrenceID: identifier
        )
        let task = Task { @MainActor [center] in
            await Self.perform(
                request,
                occurrenceID: occurrenceID,
                episodeID: episodeID,
                center: center
            )
        }
        deliveries[identifier] = task
        let observation = await task.value
        deliveries[identifier] = nil
        return observation
    }

    func cancel(occurrenceID: FeedDiscoveryOccurrenceId) {
        let identifier = occurrenceID.stableString
        deliveries.removeValue(forKey: identifier)?.cancel()
        center.remove(identifier: identifier)
    }

    func shutdown() {
        let identifiers = Array(deliveries.keys)
        for delivery in deliveries.values {
            delivery.cancel()
        }
        deliveries.removeAll()
        for identifier in identifiers {
            center.remove(identifier: identifier)
        }
    }

    private static func perform(
        _ request: CoreNotificationRequest,
        occurrenceID: FeedDiscoveryOccurrenceId,
        episodeID: EpisodeId,
        center: any CoreNotificationCentering
    ) async -> HostObservation {
        do {
            try Task.checkCancellation()
            if (await center.existingRequestIdentifiers()).contains(request.identifier) {
                return delivered(occurrenceID: occurrenceID, episodeID: episodeID)
            }
            switch await center.authorization() {
            case .authorized:
                break
            case .denied:
                return .failed(code: .permissionDenied, safeDetail: nil)
            case .notDetermined:
                guard try await center.requestAuthorization() else {
                    return .failed(code: .permissionDenied, safeDetail: nil)
                }
            case .unsupported:
                return .failed(
                    code: .platformFailure,
                    safeDetail: "Notification authorization status is unsupported"
                )
            }
            try Task.checkCancellation()
            guard !(await center.existingRequestIdentifiers()).contains(request.identifier) else {
                return delivered(occurrenceID: occurrenceID, episodeID: episodeID)
            }
            try await center.add(request)
            try Task.checkCancellation()
            return delivered(occurrenceID: occurrenceID, episodeID: episodeID)
        } catch is CancellationError {
            center.remove(identifier: request.identifier)
            return .cancelled
        } catch {
            return .failed(
                code: .platformFailure,
                safeDetail: "Notification delivery failed"
            )
        }
    }

    private static func delivered(
        occurrenceID: FeedDiscoveryOccurrenceId,
        episodeID: EpisodeId
    ) -> HostObservation {
        return .newEpisodeNotificationDelivered(
            occurrenceId: occurrenceID,
            episodeId: episodeID
        )
    }
}

@MainActor
final class UnavailableCoreNotificationHost: CoreNotificationHosting {
    func deliver(
        occurrenceID _: FeedDiscoveryOccurrenceId,
        episodeID _: EpisodeId,
        podcastID _: PodcastId,
        podcastTitle _: String,
        episodeTitle _: String
    ) async -> HostObservation {
        .failed(
            code: .platformFailure,
            safeDetail: "Native notification capability is unavailable"
        )
    }

    func cancel(occurrenceID _: FeedDiscoveryOccurrenceId) {}
    func shutdown() {}
}

@MainActor
private final class SystemCoreNotificationCenter: CoreNotificationCentering {
    private let center = UNUserNotificationCenter.current()

    func authorization() async -> CoreNotificationAuthorization {
        switch await center.notificationSettings().authorizationStatus {
        case .notDetermined: .notDetermined
        case .denied: .denied
        case .authorized, .provisional, .ephemeral: .authorized
        @unknown default: .unsupported
        }
    }

    func requestAuthorization() async throws -> Bool {
        try await center.requestAuthorization(options: [.alert, .sound, .badge])
    }

    func existingRequestIdentifiers() async -> Set<String> {
        let pending = await center.pendingNotificationRequests()
        let delivered = await center.deliveredNotifications()
        return Set(pending.map(\.identifier) + delivered.map(\.request.identifier))
    }

    func add(_ request: CoreNotificationRequest) async throws {
        let content = UNMutableNotificationContent()
        content.title = request.title
        content.body = request.body
        content.sound = .default
        content.threadIdentifier = request.threadIdentifier
        content.userInfo = [
            NotificationService.episodeIDUserInfoKey: request.episodeID,
            NotificationService.occurrenceIDUserInfoKey: request.occurrenceID,
        ]
        try await center.add(UNNotificationRequest(
            identifier: request.identifier,
            content: content,
            trigger: nil
        ))
    }

    func remove(identifier: String) {
        center.removePendingNotificationRequests(withIdentifiers: [identifier])
        center.removeDeliveredNotifications(withIdentifiers: [identifier])
    }
}
