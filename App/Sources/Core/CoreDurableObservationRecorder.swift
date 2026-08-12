import Pod0Core

/// The only native observation-delivery boundary. It persists exact transport
/// evidence first, then invokes the Rust ingress and retires evidence only on a
/// terminal Rust receipt. It never decides product state or effect outcome.
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
            guard let outbox else {
                return .retainAndRetry(requestId: observation.requestId)
            }
            do { _ = try await outbox.persistBeforeDelivery(observation) }
            catch { return .retainAndRetry(requestId: observation.requestId) }
            guard await outbox.beginDelivery(of: observation) else {
                return .retainAndRetry(requestId: observation.requestId)
            }
        }
        guard !Task.isCancelled else {
            return .retainAndRetry(requestId: observation.requestId)
        }
        let receipt = facade.recordHostObservation(observation: observation)
        if persistForRelaunch, let outbox {
            await outbox.finishDelivery(of: observation)
            _ = try? await outbox.acknowledge(receipt)
        }
        return receipt
    }

    func recordRetaining(
        _ observation: LeasedHostObservationEnvelope,
        in facade: Pod0Facade
    ) async -> HostObservationReceipt {
        guard let outbox else {
            return .retainAndRetry(requestId: observation.observation.requestId)
        }
        do { _ = try await outbox.persistBeforeDelivery(observation) }
        catch { return .retainAndRetry(requestId: observation.observation.requestId) }
        guard await outbox.beginDelivery(of: observation), !Task.isCancelled else {
            return .retainAndRetry(requestId: observation.observation.requestId)
        }
        let receipt = facade.recordLeasedHostObservation(observation: observation)
        await outbox.finishDelivery(of: observation)
        _ = try? await outbox.acknowledgeLeased(receipt)
        return receipt
    }

    func replayPending(
        in facade: Pod0Facade
    ) async -> [(HostObservationEnvelope, HostObservationReceipt)] {
        guard let outbox else { return [] }
        var replayed: [(HostObservationEnvelope, HostObservationReceipt)] = []
        for observation in await outbox.pendingObservations() {
            guard !Task.isCancelled else { return replayed }
            guard await outbox.beginDelivery(of: observation) else { continue }
            let receipt = facade.recordHostObservation(observation: observation)
            await outbox.finishDelivery(of: observation)
            guard !Task.isCancelled else { return replayed }
            _ = try? await outbox.acknowledge(receipt)
            replayed.append((observation, receipt))
        }
        return replayed
    }

    func replayPendingLeased(
        in facade: Pod0Facade
    ) async -> [(LeasedHostObservationEnvelope, HostObservationReceipt)] {
        guard let outbox else { return [] }
        var replayed: [(LeasedHostObservationEnvelope, HostObservationReceipt)] = []
        for observation in await outbox.pendingLeasedObservations() {
            guard !Task.isCancelled else { return replayed }
            guard await outbox.beginDelivery(of: observation) else { continue }
            let receipt = facade.recordLeasedHostObservation(observation: observation)
            await outbox.finishDelivery(of: observation)
            guard !Task.isCancelled else { return replayed }
            _ = try? await outbox.acknowledgeLeased(receipt)
            replayed.append((observation, receipt))
        }
        return replayed
    }
}
