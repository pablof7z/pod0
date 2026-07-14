import Foundation

actor PersistenceBackgroundWriter {
    private var pending: AppState?
    private var isDraining = false

    func enqueue(_ state: AppState, persistence: Persistence) {
        pending = state
        guard !isDraining else { return }
        isDraining = true
        Task { await drain(persistence: persistence) }
    }

    private func drain(persistence: Persistence) async {
        while let state = pending {
            pending = nil
            await Task.detached(priority: .utility) {
                persistence.write(state)
            }.value
        }
        isDraining = false
    }
}
