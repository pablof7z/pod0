import Foundation

extension Notification.Name {
    static let persistenceDidCommitWorkflowJobs = Notification.Name(
        "io.f7z.podcast.persistence.workflow-commit"
    )
}

extension Persistence {
    func publishWorkflowCommitIfNeeded(for jobs: [DesiredJob]) {
        guard !jobs.isEmpty else { return }
        NotificationCenter.default.post(
            name: .persistenceDidCommitWorkflowJobs,
            object: self
        )
    }
}
