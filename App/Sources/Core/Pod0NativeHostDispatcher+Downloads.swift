import Foundation
import Pod0Core

extension Pod0NativeHostDispatcher {
    struct ActiveDownloadRequest {
        let envelope: HostRequestEnvelope
        let delivery: Delivery
    }

    func startDownloadRequest(
        _ envelope: HostRequestEnvelope,
        delivery: @escaping Delivery
    ) {
        downloadRequests[envelope.requestId] = ActiveDownloadRequest(
            envelope: envelope,
            delivery: delivery
        )
        downloadHost.execute(envelope) { [weak self] sequence, observation in
            guard let self,
                  let active = downloadRequests[envelope.requestId]
            else { return }
            active.delivery(makeEnvelope(
                active.envelope,
                sequenceNumber: sequence,
                observedAt: now(),
                observation: observation
            ))
        }
    }

    func enqueueDownloadObservation(
        _ observation: HostObservationEnvelope,
        for envelope: HostRequestEnvelope,
        in facade: Pod0Facade,
        completion: @escaping @MainActor () -> Void
    ) {
        enqueueDownloadObservation(
            observation,
            requestID: envelope.requestId,
            in: facade,
            completion: completion
        )
    }

    func bindDownloadOrphanObservations(to facade: Pod0Facade) {
        downloadHost.installOrphanObservationSink { [weak self, weak facade] event in
            guard let self, let facade else { return }
            let observation = HostObservationEnvelope(
                requestId: event.identity.requestID,
                cancellationId: event.identity.cancellationID,
                observedRequestRevision: event.identity.observedRequestRevision,
                sequenceNumber: event.sequenceNumber,
                observedAt: UnixTimestampMilliseconds(date: now()),
                observation: event.observation
            )
            enqueueDownloadObservation(
                observation,
                requestID: event.identity.requestID,
                in: facade
            ) { [weak self, weak facade] in
                guard let self, let facade else { return }
                executePendingRequests(from: facade)
            }
        }
    }

    private func enqueueDownloadObservation(
        _ observation: HostObservationEnvelope,
        requestID: HostRequestId,
        in facade: Pod0Facade,
        completion: @escaping @MainActor () -> Void
    ) {
        pendingDownloadObservations[requestID, default: []].append(observation)
        guard downloadAcknowledgementTasks[requestID] == nil else { return }
        recordNextDownloadObservation(
            for: requestID,
            in: facade,
            completion: completion
        )
    }

    private func recordNextDownloadObservation(
        for requestID: HostRequestId,
        in facade: Pod0Facade,
        completion: @escaping @MainActor () -> Void
    ) {
        guard let observation = pendingDownloadObservations[requestID]?.first else {
            pendingDownloadObservations[requestID] = nil
            completion()
            return
        }
        let recorder = durableObservationRecorder
        let task = Task { @MainActor [weak self] in
            let receipt = await recorder.recordRetaining(
                observation,
                in: facade,
                persistForRelaunch: true
            )
            guard let self,
                  downloadAcknowledgementTasks.removeValue(forKey: requestID) != nil
            else { return }
            if pendingDownloadObservations[requestID]?.isEmpty == false {
                pendingDownloadObservations[requestID]?.removeFirst()
            }
            downloadHost.retire(
                requestID: requestID,
                observation: observation.observation,
                receipt: receipt
            )
            if Self.downloadReceiptAllowsRetirement(receipt) {
                downloadRequests[requestID] = nil
                pendingDownloadObservations[requestID] = nil
                rememberCompletion(requestID)
                completion()
                return
            }
            recordNextDownloadObservation(
                for: requestID,
                in: facade,
                completion: completion
            )
        }
        downloadAcknowledgementTasks[requestID] = task
    }

    private static func downloadReceiptAllowsRetirement(
        _ receipt: HostObservationReceipt
    ) -> Bool {
        switch receipt {
        case .persisted(_, let terminal): terminal
        case .rejected: true
        case .acceptedTransient, .retainAndRetry: false
        }
    }
}
