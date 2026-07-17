import Foundation

// MARK: - Derived Views

extension AppStateStore {

    var activeNotes: [Note] {
        state.notes.filter { !$0.deleted }
    }
}
