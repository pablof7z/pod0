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
             .removeLegacyRecallIndexArtifacts, .fetchPublisherChapters:
            let recorder = durableObservationRecorder
            Task { @MainActor in
                _ = await recorder.recordRetaining(
                    observation,
                    in: facade,
                    persistForRelaunch: false
                )
                completion()
            }
        case .executeChapterModel, .recoverChapterModelOperation,
             .executeTranscriptCapability, .executeAgentModelTurn,
             .presentAgentApproval, .executeAgentCapability, .scheduleCoreWake:
            let recorder = durableObservationRecorder
            let persistForRelaunch = switch envelope.request {
            case .executeChapterModel, .recoverChapterModelOperation,
                 .executeTranscriptCapability, .executeAgentModelTurn,
                 .presentAgentApproval, .executeAgentCapability: true
            default: false
            }
            let task = Task { @MainActor [weak self] in
                let receipt = await recorder.recordRetaining(
                    observation,
                    in: facade,
                    persistForRelaunch: persistForRelaunch
                )
                guard let self, acknowledgementTasks[envelope.requestId] != nil else { return }
                if case .retainAndRetry = receipt {
                    retainedObservationIDs.insert(envelope.requestId)
                    return
                }
                retainedObservationIDs.remove(envelope.requestId)
                acknowledgementTasks.removeValue(forKey: envelope.requestId)
                if Self.receiptAllowsRetirement(receipt, for: envelope.request) {
                    rememberCompletion(envelope.requestId)
                }
                completion()
            }
            acknowledgementTasks[envelope.requestId] = AcknowledgementTask(
                envelope: envelope,
                observation: observation,
                completion: completion,
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
        case .executeScheduledAgentTurn:
            enqueueScheduledAgentObservation(
                observation,
                for: envelope,
                in: facade,
                completion: completion
            )
        default:
            let recorder = durableObservationRecorder
            Task { @MainActor in
                _ = await recorder.recordRetaining(
                    observation,
                    in: facade,
                    persistForRelaunch: false
                )
                completion()
            }
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
        requestDrainTask?.cancel()
        requestDrainTask = nil
        requestDrainRequested = false
        observationRecoveryTask?.cancel()
        observationRecoveryTask = nil
        retainedObservationRetryTask?.cancel()
        retainedObservationRetryTask = nil
        for active in activeTasks.values {
            active.task.cancel()
        }
        activeTasks.removeAll()
        notificationHost.shutdown()
        for acknowledgement in acknowledgementTasks.values {
            acknowledgement.task.cancel()
        }
        acknowledgementTasks.removeAll()
        for acknowledgement in scheduledAgentAcknowledgementTasks.values {
            acknowledgement.cancel()
        }
        scheduledAgentAcknowledgementTasks.removeAll()
        pendingScheduledAgentObservations.removeAll()
        pendingScheduledAgentExecutions.removeAll()
        scheduledAgentObservationCompletions.removeAll()
        retainedScheduledAgentObservationIDs.removeAll()
        retainedObservationIDs.removeAll()
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
            switch request {
            case .scheduleCoreWake:
                true
            default:
                false
            }
        case .rejected: true
        case .retainAndRetry: false
        }
    }

    @discardableResult
    func retryRetainedObservations(in facade: Pod0Facade) -> Bool {
        guard retainedObservationRetryTask == nil else { return true }
        let retained = acknowledgementTasks.values.filter {
            retainedObservationIDs.contains($0.envelope.requestId)
        }
        guard !retained.isEmpty else { return false }
        let recorder = durableObservationRecorder
        retainedObservationRetryTask = Task { @MainActor [weak self] in
            var completions: [@MainActor () -> Void] = []
            for acknowledgement in retained {
                guard !Task.isCancelled else { return }
                let persistForRelaunch = switch acknowledgement.envelope.request {
                case .executeChapterModel, .recoverChapterModelOperation,
                     .executeTranscriptCapability, .executeAgentModelTurn,
                     .presentAgentApproval, .executeAgentCapability: true
                default: false
                }
                let receipt = await recorder.recordRetaining(
                    acknowledgement.observation,
                    in: facade,
                    persistForRelaunch: persistForRelaunch
                )
                guard let self,
                      retainedObservationIDs.contains(acknowledgement.envelope.requestId)
                else { continue }
                guard case .retainAndRetry = receipt else {
                    retainedObservationIDs.remove(acknowledgement.envelope.requestId)
                    acknowledgementTasks.removeValue(forKey: acknowledgement.envelope.requestId)
                    if Self.receiptAllowsRetirement(
                        receipt,
                        for: acknowledgement.envelope.request
                    ) {
                        rememberCompletion(acknowledgement.envelope.requestId)
                    }
                    completions.append(acknowledgement.completion)
                    continue
                }
            }
            guard let self else { return }
            retainedObservationRetryTask = nil
            for completion in completions { completion() }
        }
        return true
    }
}
