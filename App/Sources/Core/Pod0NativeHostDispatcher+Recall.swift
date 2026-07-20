import Pod0Core

extension Pod0NativeHostDispatcher {
    func record(
        _ observation: HostObservationEnvelope,
        for request: HostRequest,
        in facade: Pod0Facade,
        completion: @escaping @MainActor () -> Void
    ) {
        switch request {
        case .embedRecallQuery, .embedRecallSpans, .rerankRecallCandidates,
             .removeLegacyRecallIndexArtifacts:
            let recorder = recallObservationRecorder
            Task { @MainActor in
                await recorder.record(observation, in: facade)
                completion()
            }
        case .fetchPublisherChapters:
            let recorder = publisherObservationRecorder
            Task { @MainActor in
                await recorder.record(observation, in: facade)
                completion()
            }
        default:
            facade.recordHostObservation(observation: observation)
            completion()
        }
    }

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

/// Serializes recall observations away from the main actor so Rust-owned
/// SQLite work can be interrupted without blocking native rendering.
actor CoreRecallObservationRecorder {
    func record(_ observation: HostObservationEnvelope, in facade: Pod0Facade) {
        facade.recordHostObservation(observation: observation)
    }
}

/// Keeps publisher qualification and SQLite transitions off the main actor.
/// The actor serializes bounded accepted observations before the next native
/// request is admitted.
actor CorePublisherChapterObservationRecorder {
    func record(_ observation: HostObservationEnvelope, in facade: Pod0Facade) {
        facade.recordHostObservation(observation: observation)
    }
}
