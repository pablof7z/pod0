import Foundation
import Pod0Core

struct ReconciliationReport: Equatable, Sendable {
    var ensured = 0
    var adoptedArtifacts = 0
    var invalidatedArtifacts = 0
    var obsoletedJobs = 0
}

@MainActor
struct Reconciler {
    let appStore: AppStateStore
    let jobStore: JobStore
    var now: () -> Date = Date.init

    @discardableResult
    func reconcile() throws -> ReconciliationReport {
        var report = ReconciliationReport()
        let desired = DesiredStatePlanner().plan(.init(
            settings: appStore.state.settings,
            scheduledTasks: appStore.scheduledTasks,
            now: now()
        ))
        report.ensured = try jobStore.ensureJobs(desired)
        report.ensured += try jobStore.rearmSucceededRepairs(desired, now: now())
        let jobsByKey = Dictionary(
            uniqueKeysWithValues: try jobStore.allJobs().map { ($0.idempotencyKey, $0) }
        )
        for desiredJob in desired {
            guard let existing = jobsByKey[desiredJob.idempotencyKey],
                  existing.state == .blocked,
                  prerequisitesAreAvailable(for: existing) else { continue }
            try jobStore.unblock(idempotencyKey: desiredJob.idempotencyKey, now: now())
        }

        report.obsoletedJobs += try obsoleteDisabledNotifications()

        let desiredKeys = Set(desired.map(\.idempotencyKey))
        let before = try jobStore.allJobs().filter { $0.state.isActive && $0.occurrenceID == nil }.count
        try jobStore.obsoleteActiveJobs(notIn: desiredKeys)
        let after = try jobStore.allJobs().filter { $0.state.isActive && $0.occurrenceID == nil }.count
        report.obsoletedJobs += max(0, before - after)
        return report
    }

    /// Notification occurrences are authoritative history, but an undelivered
    /// occurrence must still honor the current global choice. Once disabled,
    /// terminally obsolete pending delivery so a later re-enable cannot surface
    /// a stale alert.
    private func obsoleteDisabledNotifications() throws -> Int {
        guard !appStore.state.settings.notifyOnNewEpisodes else { return 0 }
        var count = 0
        for job in try jobStore.allJobs()
            where job.kind == .newEpisodeNotification && job.state.isActive {
            try jobStore.updateActiveTerminal(id: job.id, state: .obsolete)
            count += 1
        }
        return count
    }

    private func prerequisitesAreAvailable(for job: WorkJob) -> Bool {
        switch job.kind {
        case .transcriptIngest, .transcriptIndex:
            return false
        case .metadataIndex:
            return true
        case .scheduledAgentRun:
            guard let payload = try? Self.decoder.decode(
                ScheduledRunPayload.self, from: job.payload ?? Data()
            ) else { return false }
            return LLMProviderCredentialResolver.hasAPIKey(
                for: LLMModelReference(storedID: payload.modelID).provider
            )
        case .feedDiscovery, .download, .autoDownload, .newEpisodeNotification:
            return true
        }
    }

    static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
