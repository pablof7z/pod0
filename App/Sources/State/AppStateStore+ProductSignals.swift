import Foundation

extension AppStateStore {
    func recordProductSignal(_ observation: ProductSignalObservation) {
        Task { await productSignals.record(observation) }
    }
}
