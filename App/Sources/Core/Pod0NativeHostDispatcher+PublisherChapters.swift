import Foundation
import Pod0Core

extension Pod0NativeHostDispatcher {
    func startPublisherChapterTask(
        _ envelope: HostRequestEnvelope,
        episodeID: EpisodeId,
        sourceURL: String,
        notBefore: UnixTimestampMilliseconds?,
        maximumResponseBytes: UInt64,
        delivery: @escaping Delivery
    ) {
        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            if let notBefore {
                let delay = notBefore.date.timeIntervalSince(now())
                if delay > 0 {
                    let maximumDelay = Double(UInt64.max) / 1_000_000_000
                    let nanoseconds = UInt64(min(delay, maximumDelay) * 1_000_000_000)
                    try? await Task.sleep(nanoseconds: nanoseconds)
                }
            }
            guard !Task.isCancelled else { return }
            let result = await publisherChapterHost.fetch(
                episodeID: episodeID,
                sourceURL: sourceURL,
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
