import Pod0Core

extension Pod0NativeHostDispatcher {
    func cancel(cancellationID: CancellationId) {
        let taskIDs = activeTasks.compactMap { requestID, active in
            active.envelope.cancellationId == cancellationID ? requestID : nil
        }
        for requestID in taskIDs {
            guard let active = activeTasks.removeValue(forKey: requestID) else { continue }
            active.task.cancel()
            finish(
                active.envelope,
                sequenceNumber: 0,
                observation: .cancelled,
                delivery: active.delivery
            )
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
        if let active = activeTasks[requestID],
           active.envelope.cancellationId == cancellationID {
            activeTasks.removeValue(forKey: requestID)
            active.task.cancel()
            rememberCompletion(requestID)
        }
        if let stream = playbackStreams[requestID],
           stream.envelope.cancellationId == cancellationID {
            playbackStreams.removeValue(forKey: requestID)
            rememberCompletion(requestID)
        }
    }
}
