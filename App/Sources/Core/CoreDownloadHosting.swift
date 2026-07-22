import Pod0Core

struct CoreDownloadOrphanObservation: Sendable {
    let identity: CoreDownloadTaskIdentity
    let sequenceNumber: UInt64
    let observation: HostObservation
}

@MainActor
protocol CoreDownloadHosting: AnyObject {
    typealias Delivery = @MainActor (UInt64, HostObservation) -> Void
    typealias OrphanDelivery = @MainActor (CoreDownloadOrphanObservation) -> Void

    func installOrphanObservationSink(_ sink: @escaping OrphanDelivery)
    func execute(_ envelope: HostRequestEnvelope, delivery: @escaping Delivery)
    func cancel(requestID: HostRequestId, cancellationID: CancellationId)
    func retire(
        requestID: HostRequestId,
        observation: HostObservation,
        receipt: HostObservationReceipt
    )
    func shutdown()
}

@MainActor
final class UnavailableCoreDownloadHost: CoreDownloadHosting {
    func installOrphanObservationSink(_: @escaping OrphanDelivery) {}

    func execute(_ envelope: HostRequestEnvelope, delivery: @escaping Delivery) {
        delivery(
            1,
            .failed(
                code: .platformFailure,
                safeDetail: "Native download capability is unavailable"
            )
        )
    }

    func cancel(requestID _: HostRequestId, cancellationID _: CancellationId) {}

    func retire(
        requestID _: HostRequestId,
        observation _: HostObservation,
        receipt _: HostObservationReceipt
    ) {}

    func shutdown() {}
}
