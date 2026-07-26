import Foundation
import Pod0Core

extension Pod0NativeHostDispatcher {
    struct ActiveTask {
        let envelope: HostRequestEnvelope
        let task: Task<Void, Never>
        let delivery: Delivery
    }

    struct PendingScheduledAgentExecution {
        let envelope: HostRequestEnvelope
        let execution: ScheduledAgentExecutionRequest
        let delivery: Delivery
    }

    struct AcknowledgementTask {
        let envelope: HostRequestEnvelope
        let observation: HostObservationEnvelope
        let completion: @MainActor () -> Void
        let task: Task<Void, Never>
    }

    struct PlaybackStream {
        let envelope: HostRequestEnvelope
        let episodeID: EpisodeId?
        let minimumInterval: TimeInterval
        let delivery: Delivery
        var sequenceNumber: UInt64 = 0
        var lastDeliveryAt: Date?
        var lastObservation: PlaybackLifecycleObservation?
    }
}
