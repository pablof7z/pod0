import Pod0Core

extension Pod0NativeHostDispatcher {
    func startRecallTask(
        _ envelope: HostRequestEnvelope,
        delivery: @escaping Delivery
    ) {
        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            let result = await recallHost.execute(envelope.request)
            guard activeTasks.removeValue(forKey: envelope.requestId) != nil else { return }
            let observation: HostObservation = isExpired(envelope)
                ? .failed(code: .timedOut, safeDetail: "Host request deadline expired")
                : result
            finish(
                envelope,
                sequenceNumber: 0,
                observation: observation,
                delivery: delivery
            )
        }
        activeTasks[envelope.requestId] = ActiveTask(
            envelope: envelope,
            task: task,
            delivery: delivery
        )
    }

    func shutdown() {
        for active in activeTasks.values {
            active.task.cancel()
        }
        activeTasks.removeAll()
        playbackStreams.removeAll()
    }
}
