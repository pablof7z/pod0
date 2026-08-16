import Pod0Core

extension Pod0NativeHostDispatcher {
    typealias LeasedDelivery = @MainActor (LeasedHostObservationEnvelope) -> Void

    /// Executes only the exact request paired with a Rust-persisted lease and
    /// preserves that identity on every observation returned to Rust.
    func execute(
        _ leased: LeasedHostRequestEnvelope,
        delivery: @escaping LeasedDelivery
    ) {
        executePersistedLeaseRequest(leased.request) { observation in
            delivery(LeasedHostObservationEnvelope(
                lease: leased.lease,
                observation: observation
            ))
        }
    }
}
