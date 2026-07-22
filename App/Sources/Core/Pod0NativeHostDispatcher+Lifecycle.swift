import Pod0Core

extension Pod0NativeHostDispatcher {
    func isKnown(_ requestID: HostRequestId) -> Bool {
        activeTasks[requestID] != nil
            || downloadRequests[requestID] != nil
            || playbackStreams[requestID] != nil
            || acknowledgementTasks[requestID] != nil
            || scheduledAgentAcknowledgementTasks[requestID] != nil
            || pendingScheduledAgentObservations[requestID]?.isEmpty == false
            || completedRequestIDs.contains(requestID)
    }

    func rememberCompletion(_ requestID: HostRequestId) {
        guard completedRequestIDs.insert(requestID).inserted else { return }
        completionOrder.append(requestID)
        if completionOrder.count > 256 {
            completedRequestIDs.remove(completionOrder.removeFirst())
        }
    }
}
