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
}
