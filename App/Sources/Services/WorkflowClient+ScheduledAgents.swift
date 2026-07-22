import Foundation
import Pod0Core

extension WorkflowClient {
    func attachScheduledAgentCore(_ workflows: [ScheduledAgentWorkflowProjection]) {
        coreScheduledAgentJobsByID = Dictionary(uniqueKeysWithValues: workflows.map {
            let projection = WorkflowJobProjection(scheduledAgentWorkflow: $0)
            return (projection.id, projection)
        })
        mergeJobs()
    }

    func detachScheduledAgentCore() {
        coreScheduledAgentJobsByID.removeAll()
        mergeJobs()
    }
}
