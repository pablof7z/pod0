import Foundation
import Pod0Core

extension CoreDownloadHost {
    func start(_ envelope: HostRequestEnvelope) {
        guard let identity = CoreDownloadTaskIdentity(envelope),
              let description = identity.encoded,
              case let .startEpisodeDownload(_, _, _, _, enclosureURL, resumeKey) = envelope.request,
              let url = URL(string: enclosureURL),
              let scheme = url.scheme?.lowercased(),
              scheme == "https" || scheme == "http"
        else {
            emit(
                requestID: envelope.requestId,
                sequence: 1,
                observation: .failed(
                    code: .invalidResponse,
                    safeDetail: "Download request contains an invalid URL or identity"
                )
            )
            return
        }
        identitiesByRequest[envelope.requestId] = identity
        if let staged = nativeStore.stagedFile(for: identity.attemptID),
           let count = try? staged.resourceValues(forKeys: [.fileSizeKey]).fileSize,
           count > 0 {
            emit(
                requestID: envelope.requestId,
                sequence: 2,
                observation: stagedObservation(identity, path: staged.path, byteCount: UInt64(count))
            )
            return
        }
        Task { @MainActor [weak self] in
            guard let self else { return }
            let tasks = await session.allTasks.compactMap { $0 as? URLSessionDownloadTask }
            if let task = tasks.first(where: {
                CoreDownloadTaskIdentity(encoded: $0.taskDescription) == identity
            }) {
                attach(task, identity: identity)
                task.resume()
                emitAccepted(task: task, identity: identity)
                return
            }
            let task: URLSessionDownloadTask
            if let resumeData = nativeStore.resumeData(for: resumeKey) {
                task = session.downloadTask(withResumeData: resumeData)
            } else {
                task = session.downloadTask(with: url)
            }
            task.taskDescription = description
            attach(task, identity: identity)
            emitAccepted(task: task, identity: identity)
            task.resume()
        }
    }

    func cancel(_ envelope: HostRequestEnvelope) {
        guard case let .cancelEpisodeDownload(
            episodeID,
            intentID,
            attemptID,
            expectedExternalTaskKey
        ) = envelope.request else { return }
        let cancellationIdentity = CoreDownloadTaskIdentity(
            requestID: envelope.requestId,
            cancellationID: envelope.cancellationId,
            episodeID: episodeID,
            intentID: intentID,
            attemptID: attemptID,
            inputVersion: "cancellation"
        )
        identitiesByRequest[envelope.requestId] = cancellationIdentity
        Task { @MainActor [weak self] in
            guard let self else { return }
            let tasks = await session.allTasks.compactMap { $0 as? URLSessionDownloadTask }
            let task = tasks.first { candidate in
                let identity = CoreDownloadTaskIdentity(encoded: candidate.taskDescription)
                let matchesAttempt = identity?.attemptID == attemptID
                let matchesExternal = expectedExternalTaskKey.map {
                    externalTaskKey(for: candidate) == $0
                } ?? true
                return matchesAttempt && matchesExternal
            }
            guard let task else {
                emit(
                    requestID: envelope.requestId,
                    sequence: 1,
                    observation: .downloadCancelled(
                        episodeId: episodeID,
                        intentId: intentID,
                        attemptId: attemptID
                    )
                )
                return
            }
            cancelledTaskIDs.insert(task.taskIdentifier)
            if let startIdentity = CoreDownloadTaskIdentity(encoded: task.taskDescription) {
                deliveries[startIdentity.requestID] = nil
                identitiesByRequest[startIdentity.requestID] = nil
                tasksByRequest[startIdentity.requestID] = nil
            }
            identitiesByTask[task.taskIdentifier] = nil
            Task { @MainActor [weak self, nativeStore] in
                let resumeData = await task.cancelByProducingResumeData()
                nativeStore.saveResumeData(resumeData, for: attemptID)
                guard let self else { return }
                clearProgress(for: episodeID)
                emit(
                    requestID: envelope.requestId,
                    sequence: 1,
                    observation: .downloadCancelled(
                        episodeId: episodeID,
                        intentId: intentID,
                        attemptId: attemptID
                    )
                )
            }
        }
    }

    func remove(_ envelope: HostRequestEnvelope) {
        guard case let .removeEpisodeDownloadArtifact(episodeID, artifactKey) = envelope.request
        else { return }
        do {
            try nativeStore.removeArtifact(coreStoreURL: coreStoreURL, artifactKey: artifactKey)
            emit(
                requestID: envelope.requestId,
                sequence: 1,
                observation: .downloadArtifactRemoved(
                    episodeId: episodeID,
                    artifactKey: artifactKey
                )
            )
        } catch {
            emit(
                requestID: envelope.requestId,
                sequence: 1,
                observation: .failed(
                    code: .platformFailure,
                    safeDetail: "Native artifact removal failed"
                )
            )
        }
    }

    func attach(_ task: URLSessionDownloadTask, identity: CoreDownloadTaskIdentity) {
        tasksByRequest[identity.requestID] = task
        identitiesByTask[task.taskIdentifier] = identity
        progress[identity.episodeID] = 0
        expectedBytes[identity.episodeID] = nil
        lastPublishedProgress[identity.episodeID] = 0
        lastPublishedAt[identity.episodeID] = Date()
    }

    func emitAccepted(task: URLSessionDownloadTask, identity: CoreDownloadTaskIdentity) {
        emit(
            requestID: identity.requestID,
            sequence: 1,
            observation: .downloadAccepted(
                episodeId: identity.episodeID,
                intentId: identity.intentID,
                attemptId: identity.attemptID,
                externalTaskKey: externalTaskKey(for: task),
                resumeKey: nativeStore.resumeKey(for: identity.attemptID)
            )
        )
    }

    func externalTaskKey(for task: URLSessionTask) -> String {
        let identifier = session.configuration.identifier ?? "foreground"
        return "\(identifier):\(task.taskIdentifier)"
    }

    private func stagedObservation(
        _ identity: CoreDownloadTaskIdentity,
        path: String,
        byteCount: UInt64
    ) -> HostObservation {
        .downloadStaged(
            episodeId: identity.episodeID,
            intentId: identity.intentID,
            attemptId: identity.attemptID,
            stagedFilePath: path,
            byteCount: byteCount
        )
    }
}
