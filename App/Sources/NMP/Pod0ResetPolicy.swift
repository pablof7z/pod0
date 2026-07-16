import Foundation

enum Pod0AppStateResetEffect: Sendable, Equatable {
    case preserve
    case clearPreservingSettings
    case clearAll
}

enum Pod0NMPStoreResetEffect: Sendable, Equatable {
    case preserve
    case resetAfterShutdown
}

enum Pod0ReceiptResetEffect: Sendable, Equatable {
    case preserve
    case clearAnnotations
}

enum Pod0KeychainResetEffect: Sendable, Equatable {
    case preserve
    case detachActiveHumanIdentity
    case clearAllCurrentSecrets
}

struct Pod0ResetEffects: Sendable, Equatable {
    let appState: Pod0AppStateResetEffect
    let nmpStore: Pod0NMPStoreResetEffect
    let receiptAnnotations: Pod0ReceiptResetEffect
    let keychain: Pod0KeychainResetEffect
}

extension Pod0NMPLifecycleOperation {
    var resetEffects: Pod0ResetEffects {
        switch self {
        case .switchAccount:
            Pod0ResetEffects(
                appState: .preserve,
                nmpStore: .preserve,
                receiptAnnotations: .preserve,
                keychain: .preserve
            )
        case .cachePreservingSignOut:
            Pod0ResetEffects(
                appState: .preserve,
                nmpStore: .preserve,
                receiptAnnotations: .preserve,
                keychain: .detachActiveHumanIdentity
            )
        case .clearAppDataPreservingIdentities:
            Pod0ResetEffects(
                appState: .clearPreservingSettings,
                nmpStore: .preserve,
                receiptAnnotations: .clearAnnotations,
                keychain: .preserve
            )
        case .resetNostrData:
            Pod0ResetEffects(
                appState: .preserve,
                nmpStore: .resetAfterShutdown,
                receiptAnnotations: .clearAnnotations,
                keychain: .preserve
            )
        case .resetForMutuallyUntrustedUser:
            Pod0ResetEffects(
                appState: .clearAll,
                nmpStore: .resetAfterShutdown,
                receiptAnnotations: .clearAnnotations,
                keychain: .clearAllCurrentSecrets
            )
        }
    }
}

enum Pod0DestructiveResetConfirmation: Sendable, Equatable {
    case resetNostrData
    case mutuallyUntrustedUserHandoff
}

enum Pod0ResetPolicyError: LocalizedError, Equatable {
    case confirmationRequired(Pod0DestructiveResetConfirmation)
    case keychainDeletionFailed([String])

    var errorDescription: String? {
        switch self {
        case .confirmationRequired(let confirmation):
            "Destructive reset requires explicit confirmation for \(confirmation)."
        case .keychainDeletionFailed(let labels):
            "Could not clear these Keychain items: \(labels.joined(separator: ", "))."
        }
    }
}

@MainActor
struct Pod0ResetActions {
    let cachePreservingSignOut: () throws -> Void
    let clearAppDataPreservingIdentities: () -> Void
    let clearAllAppState: () -> Void
    let resetNostrStoreAfterShutdown: () throws -> Void
    let clearReceiptAnnotations: () -> Void
    let clearAllCurrentKeychainSecrets: () throws -> Void
}

/// Executes Pod0-owned reset policy. NMP is asked only to shut down and reset
/// its canonical store; it never decides which product or Keychain data Pod0
/// retains. Destructive store resets require an exact confirmation value.
@MainActor
final class Pod0ResetCoordinator {
    private let actions: Pod0ResetActions

    init(actions: Pod0ResetActions) {
        self.actions = actions
    }

    static func clearAppDataPreservingIdentities(
        appState: AppStateStore,
        receiptStore: any EpisodeCommentReceiptStore = UserDefaultsEpisodeCommentReceiptStore()
    ) {
        receiptStore.removeAll()
        appState.clearAllData()
    }

    #if canImport(NMP)
    convenience init(
        appState: AppStateStore,
        identity: UserIdentityStore,
        composition: Pod0NMPComposition,
        receiptStore: any EpisodeCommentReceiptStore = UserDefaultsEpisodeCommentReceiptStore(),
        keychainResetter: any Pod0KeychainResetting = Pod0CurrentKeychainResetter()
    ) {
        self.init(actions: Pod0ResetActions(
            cachePreservingSignOut: { try identity.cachePreservingSignOut() },
            clearAppDataPreservingIdentities: {
                Self.clearAppDataPreservingIdentities(
                    appState: appState,
                    receiptStore: receiptStore
                )
            },
            clearAllAppState: { appState.clearAppStateForMutuallyUntrustedUser() },
            resetNostrStoreAfterShutdown: {
                composition.shutdown()
                do {
                    try composition.resetStoreAfterShutdown()
                    identity.markNMPCompositionStoppedForReset()
                } catch {
                    identity.markNMPCompositionStoppedForReset()
                    throw error
                }
            },
            clearReceiptAnnotations: { receiptStore.removeAll() },
            clearAllCurrentKeychainSecrets: {
                try keychainResetter.clearAllCurrentSecrets()
                identity.finishMutuallyUntrustedUserReset()
            }
        ))
    }
    #endif

    func cachePreservingSignOut() throws {
        try actions.cachePreservingSignOut()
    }

    func clearAppDataPreservingIdentities() {
        actions.clearAppDataPreservingIdentities()
    }

    func resetNostrData(confirmation: Pod0DestructiveResetConfirmation?) throws {
        guard confirmation == .resetNostrData else {
            throw Pod0ResetPolicyError.confirmationRequired(.resetNostrData)
        }
        try actions.resetNostrStoreAfterShutdown()
        actions.clearReceiptAnnotations()
    }

    func resetForMutuallyUntrustedUser(
        confirmation: Pod0DestructiveResetConfirmation?
    ) throws {
        guard confirmation == .mutuallyUntrustedUserHandoff else {
            throw Pod0ResetPolicyError.confirmationRequired(.mutuallyUntrustedUserHandoff)
        }
        // Do not erase Pod0 product state until the canonical store is gone and
        // every current secret was successfully removed. A failed attempt keeps
        // the remaining evidence available and can retry the idempotent reset.
        try actions.resetNostrStoreAfterShutdown()
        try actions.clearAllCurrentKeychainSecrets()
        actions.clearReceiptAnnotations()
        actions.clearAllAppState()
    }
}
