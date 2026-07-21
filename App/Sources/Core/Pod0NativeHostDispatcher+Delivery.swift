import Foundation
import Pod0Core

extension Pod0NativeHostDispatcher {
    func isExpired(_ envelope: HostRequestEnvelope) -> Bool {
        envelope.deadlineAt.map { $0.date <= now() } ?? false
    }

    func finish(
        _ envelope: HostRequestEnvelope,
        sequenceNumber: UInt64,
        observation: HostObservation,
        delivery: Delivery,
        remember: Bool = true
    ) {
        if remember {
            rememberCompletion(envelope.requestId)
        }
        delivery(makeEnvelope(
            envelope,
            sequenceNumber: sequenceNumber,
            observedAt: now(),
            observation: observation
        ))
    }

    func makeEnvelope(
        _ request: HostRequestEnvelope,
        sequenceNumber: UInt64,
        observedAt: Date,
        observation: HostObservation
    ) -> HostObservationEnvelope {
        HostObservationEnvelope(
            requestId: request.requestId,
            cancellationId: request.cancellationId,
            observedRequestRevision: request.issuedRevision,
            sequenceNumber: sequenceNumber,
            observedAt: UnixTimestampMilliseconds(date: observedAt),
            observation: observation
        )
    }
}
