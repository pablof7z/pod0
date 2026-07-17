import Foundation

struct BackgroundDownloadTaskFact: Sendable, Equatable {
    let taskIdentifier: Int
    let jobID: UUID?
    let episodeID: UUID?
}

enum DownloadReconciliationAction: Sendable, Equatable {
    case attach(taskIdentifier: Int, jobID: UUID, episodeID: UUID)
    case cancelOrphan(taskIdentifier: Int)
    case requeueMissingTask(jobID: UUID)
}

/// Pure launch-time comparison used by the live URLSession adapter and fakes.
struct DownloadReconciliationPlanner: Sendable {
    func plan(
        tasks: [BackgroundDownloadTaskFact],
        jobs: [WorkJob]
    ) -> [DownloadReconciliationAction] {
        let jobsByID = Dictionary(uniqueKeysWithValues: jobs.map { ($0.id, $0) })
        var backed: Set<UUID> = []
        var actions: [DownloadReconciliationAction] = []
        for task in tasks.sorted(by: { $0.taskIdentifier < $1.taskIdentifier }) {
            guard let jobID = task.jobID,
                  let episodeID = task.episodeID,
                  let job = jobsByID[jobID],
                  job.kind == .download,
                  job.state.isActive,
                  job.subjectID == episodeID else {
                actions.append(.cancelOrphan(taskIdentifier: task.taskIdentifier))
                continue
            }
            backed.insert(jobID)
            actions.append(.attach(
                taskIdentifier: task.taskIdentifier,
                jobID: jobID,
                episodeID: episodeID
            ))
        }
        for job in jobs.sorted(by: { $0.id.uuidString < $1.id.uuidString })
            where job.kind == .download
                && (job.state == .leased || job.state == .running)
                && !backed.contains(job.id) {
            actions.append(.requeueMissingTask(jobID: job.id))
        }
        return actions
    }
}
