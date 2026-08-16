import Pod0Core

extension Pod0NativeHostDispatcher {
    func startLibraryNetworkTask(
        _ envelope: HostRequestEnvelope,
        workflowCommandID: CommandId,
        step: LibraryNetworkStep,
        url: String,
        accept: String,
        maximumResponseBytes: UInt64,
        delivery: @escaping Delivery
    ) {
        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            let result = await libraryNetworkHost.fetch(
                workflowCommandID: workflowCommandID,
                step: step,
                url: url,
                accept: accept,
                maximumResponseBytes: maximumResponseBytes,
                deadline: envelope.deadlineAt?.date
            )
            guard activeTasks.removeValue(forKey: envelope.requestId) != nil else { return }
            finish(
                envelope,
                sequenceNumber: 0,
                observation: isExpired(envelope)
                    ? .failed(code: .timedOut, safeDetail: "Host request deadline expired")
                    : result,
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
