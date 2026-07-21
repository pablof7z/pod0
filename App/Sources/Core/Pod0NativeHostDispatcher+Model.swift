import Foundation
import Pod0Core

extension Pod0NativeHostDispatcher {
    func startObservationRecovery(
        from facade: Pod0Facade,
        maximumCount: UInt16
    ) {
        guard observationRecoveryTask == nil else { return }
        let recorder = durableObservationRecorder
        observationRecoveryTask = Task { @MainActor [weak self] in
            await recorder.replayPending(in: facade)
            guard let self, !Task.isCancelled else { return }
            observationRecoveryReady = true
            observationRecoveryTask = nil
            executePendingRequests(from: facade, maximumCount: maximumCount)
        }
    }

    func startChapterModelTask(
        _ envelope: HostRequestEnvelope,
        delivery: @escaping Delivery
    ) {
        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            let result = await chapterModelHost.execute(envelope.request)
            guard activeTasks.removeValue(forKey: envelope.requestId) != nil else { return }
            let observation: HostObservation = isExpired(envelope)
                ? .failed(code: .timedOut, safeDetail: "Host request deadline expired")
                : result
            finish(
                envelope,
                sequenceNumber: 0,
                observation: observation,
                delivery: delivery,
                remember: false
            )
        }
        activeTasks[envelope.requestId] = ActiveTask(
            envelope: envelope,
            task: task,
            delivery: delivery
        )
    }

    func startCoreWakeTask(
        _ envelope: HostRequestEnvelope,
        wakeAt: UnixTimestampMilliseconds,
        reason: CoreWakeReason,
        delivery: @escaping Delivery
    ) {
        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            let delay = max(0, wakeAt.date.timeIntervalSince(now()))
            do {
                try await Task.sleep(for: .seconds(delay))
            } catch {
                return
            }
            guard activeTasks.removeValue(forKey: envelope.requestId) != nil else { return }
            finish(
                envelope,
                sequenceNumber: 0,
                observation: .coreWakeReached(reason: reason),
                delivery: delivery,
                remember: false
            )
        }
        activeTasks[envelope.requestId] = ActiveTask(
            envelope: envelope,
            task: task,
            delivery: delivery
        )
    }
}
