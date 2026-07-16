import Foundation

#if canImport(NMP)
import NMP

@MainActor
extension Pod0HumanIdentityLifecycle {
    /// A same-process import retains NMP's exact registration and can sign
    /// out. A cold-restored account has no public exact-detach handle (#589).
    func cachePreservingSignOut() throws {
        if let localRegistration {
            try signOutLocal(registration: localRegistration)
            return
        }
        if remoteConnection != nil {
            try signOutRemote()
            return
        }
        if let blocker {
            try block(blocker)
        }
        if try isVerifiedRestoredLocal() {
            try block(.restoredLocalDetachUnsupported(issue: 589))
        }
        try cleanStaleSelectionOrRemainSignedOut()
    }

    func rollbackFailedRegistration(
        _ registration: NMPAccountRegistration,
        original: Error,
        resetActive: Bool,
        clearCatalog: Bool
    ) -> Error {
        var failures: [String] = []
        if resetActive {
            capture(&failures) { try engineAccess.engine.setActiveAccount(nil) }
        }
        do {
            if try !removeExactAccount(registration) {
                failures.append("NMP rejected the retained registration as stale.")
            }
        } catch {
            failures.append(error.localizedDescription)
        }
        if clearCatalog {
            capture(&failures) { try catalogStorage.clear() }
        }
        return surfaced(original: original, rollbackFailures: failures)
    }

    func rollbackFailedBunkerConnection(
        original: Error,
        resetActive: Bool,
        clearCatalog: Bool
    ) -> Error {
        var failures: [String] = []
        if resetActive {
            capture(&failures) { try engineAccess.engine.setActiveAccount(nil) }
        }
        if clearCatalog {
            capture(&failures) { try catalogStorage.clear() }
        }
        return surfaced(original: original, rollbackFailures: failures)
    }

    private func signOutLocal(registration: NMPAccountRegistration) throws {
        let previousActive = try engineAccess.engine.activeAccount()
        let previousCatalog = try catalogStorage.load()
        try catalogStorage.clear()
        do {
            try engineAccess.engine.setActiveAccount(nil)
            guard try removeExactAccount(registration) else {
                throw Pod0HumanIdentityError.rollbackFailed(
                    original: "Exact local sign-out was requested.",
                    rollback: "NMP rejected the retained registration as stale."
                )
            }
        } catch {
            if case Pod0HumanIdentityError.checkpointRecoveryFailed = error {
                localRegistration = nil
                blocker = .identitySwitchUnsupported
                state = .failed(error.localizedDescription)
                throw error
            }
            let surfaced = restoreProjection(
                active: previousActive,
                catalog: previousCatalog,
                original: error
            )
            state = .failed(surfaced.localizedDescription)
            throw surfaced
        }
        localRegistration = nil
        restoredLocalPublicKey = nil
        blocker = nil
        state = .signedOut
    }

    private func signOutRemote() throws {
        let previousActive = try engineAccess.engine.activeAccount()
        let previousCatalog = try catalogStorage.load()
        try catalogStorage.clear()
        do {
            try engineAccess.engine.setActiveAccount(nil)
        } catch {
            let surfaced = restoreProjection(
                active: previousActive,
                catalog: previousCatalog,
                original: error
            )
            state = .failed(surfaced.localizedDescription)
            throw surfaced
        }
        remoteConnection?.close()
        remoteConnection = nil
        restoredLocalPublicKey = nil
        blocker = nil
        state = .signedOut
    }

    private func isVerifiedRestoredLocal() throws -> Bool {
        guard let restoredLocalPublicKey,
              case .ready(let statePublicKey) = state,
              statePublicKey == restoredLocalPublicKey,
              let catalog = try catalogStorage.load(),
              catalog.selectedRole == .human,
              let entry = catalog.entry(for: .human),
              case .localKey = entry.capability,
              entry.expectedPublicKey == restoredLocalPublicKey,
              try engineAccess.engine.activeAccount() == restoredLocalPublicKey else {
            return false
        }
        return true
    }

    private func cleanStaleSelectionOrRemainSignedOut() throws {
        if try engineAccess.engine.activeAccount() != nil {
            try engineAccess.engine.setActiveAccount(nil)
            try block(.orphanedRestoredLocal(issue: 589))
        }
        if try catalogStorage.load() != nil {
            try catalogStorage.clear()
        }
        restoredLocalPublicKey = nil
        blocker = nil
        state = .signedOut
    }

    private func restoreProjection(
        active: String?,
        catalog: Pod0IdentityCatalog?,
        original: Error
    ) -> Error {
        var failures: [String] = []
        capture(&failures) { try engineAccess.engine.setActiveAccount(active) }
        if let catalog {
            capture(&failures) { try catalogStorage.save(catalog) }
        }
        return surfaced(original: original, rollbackFailures: failures)
    }

    private func removeExactAccount(_ registration: NMPAccountRegistration) throws -> Bool {
        do {
            return try engineAccess.engine.removeAccount(registration)
        } catch is NMPAccountCheckpointClearError {
            do {
                try engineAccess.engine.clearPersistedAccount()
                return true
            } catch {
                throw Pod0HumanIdentityError.checkpointRecoveryFailed(error.localizedDescription)
            }
        }
    }

    private func capture(_ failures: inout [String], operation: () throws -> Void) {
        do {
            try operation()
        } catch {
            failures.append(error.localizedDescription)
        }
    }

    private func surfaced(original: Error, rollbackFailures: [String]) -> Error {
        guard !rollbackFailures.isEmpty else { return original }
        return Pod0HumanIdentityError.rollbackFailed(
            original: original.localizedDescription,
            rollback: rollbackFailures.joined(separator: "; ")
        )
    }
}
#endif
