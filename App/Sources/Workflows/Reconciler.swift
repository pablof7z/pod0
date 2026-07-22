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
        report.obsoletedJobs += try obsoleteDisabledNotifications()
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

}
