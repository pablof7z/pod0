import Pod0Core

extension Pod0NativeHostDispatcher {
    func startFeedTask(
        _ envelope: HostRequestEnvelope,
        feedURL: String,
        entityTag: String?,
        lastModified: String?,
        maximumResponseBytes: UInt64,
        delivery: @escaping Delivery
    ) {
        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            let result = await feedHost.fetch(
                feedURL: feedURL,
                entityTag: entityTag,
                lastModified: lastModified,
                maximumResponseBytes: maximumResponseBytes,
                deadline: envelope.deadlineAt?.date
            )
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
}
