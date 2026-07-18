import Foundation

extension AppStateStore {
    static let workflowEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()

    /// Records native download/notification effects for episodes that the
    /// shared core has already committed. Feed identity and episode metadata
    /// remain Rust-owned; this method only stages platform workflow intent.
    func recordSharedFeedDiscovery(
        podcastID: UUID,
        episodeIDs: [UUID],
        notificationDiscoveredAt: Date?
    ) {
        let jobs = feedDiscoveryJobs(
            podcastID: podcastID,
            episodeIDs: episodeIDs,
            episodes: state.episodes,
            evaluateAutoDownload: true,
            notificationDiscoveredAt: notificationDiscoveredAt
        )
        guard !jobs.isEmpty else { return }
        performMutationBatch {
            mutateState(ensuring: jobs) { _ in }
        }
        WorkflowRuntime.shared.wake()
    }

    func feedDiscoveryJobs(
        podcastID: UUID,
        episodeIDs: [UUID],
        episodes: [Episode],
        evaluateAutoDownload: Bool,
        notificationDiscoveredAt: Date?
    ) -> [DesiredJob] {
        let inputs = episodeIDs.compactMap { id -> FeedDiscoveryPayload.EpisodeInput? in
            guard let episode = episodes.first(where: { $0.id == id }) else { return nil }
            return .init(
                episodeID: id,
                inputVersion: DesiredStatePlanner.audioVersion(episode),
                pubDate: episode.pubDate,
                title: episode.title
            )
        }.sorted { $0.episodeID.uuidString < $1.episodeID.uuidString }
        let recordsDiscovery = evaluateAutoDownload || notificationDiscoveredAt != nil
        guard !inputs.isEmpty, recordsDiscovery else { return [] }

        let batchVersion = ArtifactRepository.version(parts: inputs.flatMap {
            [$0.episodeID.uuidString, $0.inputVersion]
        })
        let occurrence = "discovery:\(podcastID.uuidString):\(batchVersion)"
        let payload = FeedDiscoveryPayload(
            podcastID: podcastID,
            occurrenceID: occurrence,
            discoveredAt: notificationDiscoveredAt ?? Date(),
            episodes: inputs,
            autoDownloadPolicy: evaluateAutoDownload
                ? effectiveAutoDownload(forPodcast: podcastID)
                : nil,
            notificationsEnabled: notificationDiscoveredAt != nil,
            policyVersion: "feed-policy-v1"
        )
        return [DesiredJob(
            idempotencyKey: occurrence,
            kind: .feedDiscovery,
            subjectID: podcastID,
            inputVersion: batchVersion,
            occurrenceID: occurrence,
            payload: try? Self.workflowEncoder.encode(payload),
            priority: 40,
            resourceClass: .planning,
            maxAttempts: 8
        )]
    }
}
