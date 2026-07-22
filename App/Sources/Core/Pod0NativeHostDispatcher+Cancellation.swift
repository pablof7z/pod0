import Pod0Core

extension Pod0NativeHostDispatcher {
    func cancel(cancellationID: CancellationId) {
        let downloadIDs = downloadRequests.compactMap { requestID, active in
            active.envelope.cancellationId == cancellationID ? requestID : nil
        }
        for requestID in downloadIDs {
            guard let active = downloadRequests.removeValue(forKey: requestID) else { continue }
            downloadHost.cancel(
                requestID: requestID,
                cancellationID: active.envelope.cancellationId
            )
            pendingDownloadObservations[requestID] = nil
            downloadAcknowledgementTasks.removeValue(forKey: requestID)?.cancel()
            rememberCompletion(requestID)
        }
        let pendingScheduledIDs = pendingScheduledAgentExecutions.compactMap {
            requestID, pending in
            pending.envelope.cancellationId == cancellationID ? requestID : nil
        }
        for requestID in pendingScheduledIDs {
            _ = cancelPendingScheduledAgentExecution(
                requestID: requestID,
                cancellationID: cancellationID
            )
        }
        let taskIDs = activeTasks.compactMap { requestID, active in
            active.envelope.cancellationId == cancellationID ? requestID : nil
        }
        for requestID in taskIDs {
            guard let active = activeTasks.removeValue(forKey: requestID) else { continue }
            if !cancelScheduledAgentTask(active) {
                active.task.cancel()
                finish(
                    active.envelope,
                    sequenceNumber: 0,
                    observation: .cancelled,
                    delivery: active.delivery
                )
            }
        }

        let streamIDs = playbackStreams.compactMap { requestID, stream in
            stream.envelope.cancellationId == cancellationID ? requestID : nil
        }
        for requestID in streamIDs {
            guard let stream = playbackStreams.removeValue(forKey: requestID) else { continue }
            finish(
                stream.envelope,
                sequenceNumber: stream.sequenceNumber + 1,
                observation: .cancelled,
                delivery: stream.delivery
            )
        }
    }

    func cancel(requestID: HostRequestId, cancellationID: CancellationId) {
        if cancelPendingScheduledAgentExecution(
            requestID: requestID,
            cancellationID: cancellationID
        ) {
            return
        }
        if let active = downloadRequests[requestID],
           active.envelope.cancellationId == cancellationID {
            downloadRequests[requestID] = nil
            pendingDownloadObservations[requestID] = nil
            downloadAcknowledgementTasks.removeValue(forKey: requestID)?.cancel()
            downloadHost.cancel(requestID: requestID, cancellationID: cancellationID)
            rememberCompletion(requestID)
        }
        if let active = activeTasks[requestID],
           active.envelope.cancellationId == cancellationID {
            activeTasks.removeValue(forKey: requestID)
            if !cancelScheduledAgentTask(active) {
                active.task.cancel()
                rememberCompletion(requestID)
            }
        }
        if let stream = playbackStreams[requestID],
           stream.envelope.cancellationId == cancellationID {
            playbackStreams.removeValue(forKey: requestID)
            rememberCompletion(requestID)
        }
    }
}
