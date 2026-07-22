import Pod0Core

extension Pod0NativeHostDispatcher {
    func record(
        _ observation: HostObservationEnvelope,
        for envelope: HostRequestEnvelope,
        in facade: Pod0Facade,
        completion: @escaping @MainActor () -> Void
    ) {
        switch envelope.request {
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
        case .executeChapterModel, .recoverChapterModelOperation,
             .executeTranscriptCapability, .scheduleCoreWake:
            let recorder = durableObservationRecorder
            let persistForRelaunch = switch envelope.request {
            case .executeChapterModel, .recoverChapterModelOperation,
                 .executeTranscriptCapability: true
            default: false
            }
            let task = Task { @MainActor [weak self] in
                let receipt = await recorder.recordRetaining(
                    observation,
                    in: facade,
                    persistForRelaunch: persistForRelaunch
                )
                guard let self,
                      acknowledgementTasks.removeValue(forKey: envelope.requestId) != nil
                else { return }
                if Self.receiptAllowsRetirement(receipt, for: envelope.request) {
                    rememberCompletion(envelope.requestId)
                }
                completion()
            }
            acknowledgementTasks[envelope.requestId] = AcknowledgementTask(
                envelope: envelope,
                task: task
            )
        case .startEpisodeDownload, .cancelEpisodeDownload,
             .removeEpisodeDownloadArtifact:
            enqueueDownloadObservation(
                observation,
                for: envelope,
                in: facade,
                completion: completion
            )
        default:
            _ = facade.recordHostObservation(observation: observation)
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
        observationRecoveryTask?.cancel()
        observationRecoveryTask = nil
        for active in activeTasks.values {
            active.task.cancel()
        }
        activeTasks.removeAll()
        for acknowledgement in acknowledgementTasks.values {
            acknowledgement.task.cancel()
        }
        acknowledgementTasks.removeAll()
        for acknowledgement in downloadAcknowledgementTasks.values {
            acknowledgement.cancel()
        }
        downloadAcknowledgementTasks.removeAll()
        playbackStreams.removeAll()
        downloadHost.shutdown()
        downloadRequests.removeAll()
        pendingDownloadObservations.removeAll()
    }

    static func receiptAllowsRetirement(
        _ receipt: HostObservationReceipt,
        for request: HostRequest
    ) -> Bool {
        switch receipt {
        case .persisted(_, let terminal): terminal
        case .acceptedTransient:
            if case .scheduleCoreWake = request { true } else { false }
        case .rejected: true
        case .retainAndRetry: false
        }
    }
}

/// Serializes recall observations away from the main actor so Rust-owned
/// SQLite work can be interrupted without blocking native rendering.
actor CoreRecallObservationRecorder {
    func record(_ observation: HostObservationEnvelope, in facade: Pod0Facade) {
        _ = facade.recordHostObservation(observation: observation)
    }
}

/// Keeps publisher qualification and SQLite transitions off the main actor.
/// The actor serializes bounded accepted observations before the next native
/// request is admitted.
actor CorePublisherChapterObservationRecorder {
    func record(_ observation: HostObservationEnvelope, in facade: Pod0Facade) {
        _ = facade.recordHostObservation(observation: observation)
    }
}

/// Retains the exact observation in actor state until Rust acknowledges that
/// it has been persisted or rejects it as terminally unusable.
actor CoreDurableObservationRecorder {
    private let outbox: NativeHostObservationOutbox?

    init(outbox: NativeHostObservationOutbox?) {
        self.outbox = outbox
    }

    func recordRetaining(
        _ observation: HostObservationEnvelope,
        in facade: Pod0Facade,
        persistForRelaunch: Bool
    ) async -> HostObservationReceipt {
        if persistForRelaunch {
            guard await persist(observation) else {
                return .retainAndRetry(requestId: observation.requestId)
            }
        }
        let receipt = await deliverRetaining(observation, in: facade)
        if persistForRelaunch, let outbox {
            _ = try? await outbox.acknowledge(receipt)
        }
        return receipt
    }

    func replayPending(
        in facade: Pod0Facade
    ) async -> [(HostObservationEnvelope, HostObservationReceipt)] {
        guard let outbox else { return [] }
        var replayed: [(HostObservationEnvelope, HostObservationReceipt)] = []
        for observation in await outbox.pendingObservations() {
            guard !Task.isCancelled else { return replayed }
            let receipt = await deliverRetaining(observation, in: facade)
            guard !Task.isCancelled else { return replayed }
            _ = try? await outbox.acknowledge(receipt)
            replayed.append((observation, receipt))
        }
        return replayed
    }

    private func persist(_ observation: HostObservationEnvelope) async -> Bool {
        guard let outbox else { return false }
        var delay = Duration.milliseconds(25)
        while !Task.isCancelled {
            do {
                _ = try await outbox.persistBeforeDelivery(observation)
                return true
            } catch {
                guard await pause(&delay) else { return false }
            }
        }
        return false
    }

    private func deliverRetaining(
        _ observation: HostObservationEnvelope,
        in facade: Pod0Facade
    ) async -> HostObservationReceipt {
        var delay = Duration.milliseconds(25)
        while !Task.isCancelled {
            let receipt = facade.recordHostObservation(observation: observation)
            guard case .retainAndRetry = receipt else { return receipt }
            guard await pause(&delay) else { break }
        }
        return .retainAndRetry(requestId: observation.requestId)
    }

    private func pause(_ delay: inout Duration) async -> Bool {
        do {
            try await Task.sleep(for: delay)
        } catch {
            return false
        }
        delay = min(delay * 2, .seconds(2))
        return true
    }
}
