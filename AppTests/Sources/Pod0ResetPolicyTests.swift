import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class Pod0ResetPolicyTests: XCTestCase {
    func testOperationsDeclareDistinctEffects() {
        XCTAssertEqual(
            Pod0NMPLifecycleOperation.cachePreservingSignOut.resetEffects,
            effects(.preserve, .preserve, .preserve, .detachActiveHumanIdentity)
        )
        XCTAssertEqual(
            Pod0NMPLifecycleOperation.clearAppDataPreservingIdentities.resetEffects,
            effects(.clearPreservingSettings, .preserve, .clearAnnotations, .preserve)
        )
        XCTAssertEqual(
            Pod0NMPLifecycleOperation.resetNostrData.resetEffects,
            effects(.preserve, .resetAfterShutdown, .clearAnnotations, .preserve)
        )
        XCTAssertEqual(
            Pod0NMPLifecycleOperation.resetForMutuallyUntrustedUser.resetEffects,
            effects(.clearAll, .resetAfterShutdown, .clearAnnotations, .clearAllCurrentSecrets)
        )
    }

    func testCachePreservingSignOutTouchesOnlyIdentityLifecycle() throws {
        let probe = ResetActionProbe()
        try coordinator(probe).cachePreservingSignOut()
        XCTAssertEqual(probe.events, ["sign-out"])
    }

    func testClearAppDataPreservesSettingsAndClearsReceiptAnnotations() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        let store = AppStateTestSupport.makeIsolatedStore(fileURL: fileURL).store
        var settings = store.state.settings
        settings.hasCompletedOnboarding = true
        store.updateSettings(settings)
        _ = store.addNote(text: "private note")

        let receipts = ResetMemoryReceiptStore()
        receipts.save(PendingEpisodeCommentReceipt(
            receiptID: 7,
            target: .episode(guid: "episode"),
            eventID: "event",
            submittedAt: Date()
        ))

        Pod0ResetCoordinator.clearAppDataPreservingIdentities(
            appState: store,
            receiptStore: receipts
        )

        XCTAssertTrue(store.state.notes.isEmpty)
        XCTAssertTrue(store.state.settings.hasCompletedOnboarding)
        XCTAssertTrue(receipts.records(for: .episode(guid: "episode")).isEmpty)
    }

    func testUntrustedUserAppStateResetDoesNotPreserveSettings() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        let store = AppStateTestSupport.makeIsolatedStore(fileURL: fileURL).store
        var settings = store.state.settings
        settings.hasCompletedOnboarding = true
        store.updateSettings(settings)
        _ = store.addNote(text: "private note")

        store.clearAppStateForMutuallyUntrustedUser()

        XCTAssertTrue(store.state.notes.isEmpty)
        XCTAssertFalse(store.state.settings.hasCompletedOnboarding)
    }

    func testNostrResetRequiresExactConfirmationThenClearsAnnotations() throws {
        let probe = ResetActionProbe()
        let resetter = coordinator(probe)

        XCTAssertThrowsError(try resetter.resetNostrData(confirmation: nil)) { error in
            XCTAssertEqual(
                error as? Pod0ResetPolicyError,
                .confirmationRequired(.resetNostrData)
            )
        }
        XCTAssertTrue(probe.events.isEmpty)

        try resetter.resetNostrData(confirmation: .resetNostrData)
        XCTAssertEqual(probe.events, ["reset-nmp", "clear-receipts"])
    }

    func testUntrustedUserResetOrdersStoreAndKeychainBeforeProductDeletion() throws {
        let probe = ResetActionProbe()

        try coordinator(probe).resetForMutuallyUntrustedUser(
            confirmation: .mutuallyUntrustedUserHandoff
        )

        XCTAssertEqual(
            probe.events,
            ["reset-nmp", "clear-keychain", "clear-receipts", "clear-app-state"]
        )
    }

    func testFailedStoreResetPreservesDependentDataAndCanRetry() throws {
        let probe = ResetActionProbe()
        probe.remainingStoreResetFailures = 1
        let resetter = coordinator(probe)

        XCTAssertThrowsError(try resetter.resetForMutuallyUntrustedUser(
            confirmation: .mutuallyUntrustedUserHandoff
        ))
        XCTAssertEqual(probe.events, ["reset-nmp"])

        probe.events.removeAll()
        try resetter.resetForMutuallyUntrustedUser(
            confirmation: .mutuallyUntrustedUserHandoff
        )
        XCTAssertEqual(
            probe.events,
            ["reset-nmp", "clear-keychain", "clear-receipts", "clear-app-state"]
        )
    }

    func testFailedKeychainResetPreservesDependentDataAndCanRetry() throws {
        let probe = ResetActionProbe()
        probe.remainingKeychainResetFailures = 1
        let resetter = coordinator(probe)

        XCTAssertThrowsError(try resetter.resetForMutuallyUntrustedUser(
            confirmation: .mutuallyUntrustedUserHandoff
        ))
        XCTAssertEqual(probe.events, ["reset-nmp", "clear-keychain"])

        probe.events.removeAll()
        try resetter.resetForMutuallyUntrustedUser(
            confirmation: .mutuallyUntrustedUserHandoff
        )
        XCTAssertEqual(
            probe.events,
            ["reset-nmp", "clear-keychain", "clear-receipts", "clear-app-state"]
        )
    }

    func testKeychainResetAttemptsEveryCurrentDeletionAndReportsFailures() {
        var attempted: [String] = []
        let resetter = Pod0CurrentKeychainResetter(deletions: [
            Pod0CurrentSecretDeletion(label: "first") {
                attempted.append("first")
                throw ResetProbeError.injected
            },
            Pod0CurrentSecretDeletion(label: "second") {
                attempted.append("second")
            },
        ])

        XCTAssertThrowsError(try resetter.clearAllCurrentSecrets()) { error in
            XCTAssertEqual(
                error as? Pod0ResetPolicyError,
                .keychainDeletionFailed(["first"])
            )
        }
        XCTAssertEqual(attempted, ["first", "second"])
    }

    private func effects(
        _ appState: Pod0AppStateResetEffect,
        _ nmpStore: Pod0NMPStoreResetEffect,
        _ receipts: Pod0ReceiptResetEffect,
        _ keychain: Pod0KeychainResetEffect
    ) -> Pod0ResetEffects {
        Pod0ResetEffects(
            appState: appState,
            nmpStore: nmpStore,
            receiptAnnotations: receipts,
            keychain: keychain
        )
    }

    private func coordinator(_ probe: ResetActionProbe) -> Pod0ResetCoordinator {
        Pod0ResetCoordinator(actions: Pod0ResetActions(
            cachePreservingSignOut: { probe.events.append("sign-out") },
            clearAppDataPreservingIdentities: { probe.events.append("clear-app-data") },
            clearAllAppState: { probe.events.append("clear-app-state") },
            resetNostrStoreAfterShutdown: {
                probe.events.append("reset-nmp")
                if probe.remainingStoreResetFailures > 0 {
                    probe.remainingStoreResetFailures -= 1
                    throw ResetProbeError.injected
                }
            },
            clearReceiptAnnotations: { probe.events.append("clear-receipts") },
            clearAllCurrentKeychainSecrets: {
                probe.events.append("clear-keychain")
                if probe.remainingKeychainResetFailures > 0 {
                    probe.remainingKeychainResetFailures -= 1
                    throw ResetProbeError.injected
                }
            }
        ))
    }
}

@MainActor
private final class ResetActionProbe {
    var events: [String] = []
    var remainingStoreResetFailures = 0
    var remainingKeychainResetFailures = 0
}

private enum ResetProbeError: Error {
    case injected
}

private final class ResetMemoryReceiptStore: EpisodeCommentReceiptStore, @unchecked Sendable {
    private let lock = NSLock()
    private var values: [PendingEpisodeCommentReceipt] = []

    func records(for target: CommentTarget) -> [PendingEpisodeCommentReceipt] {
        lock.withLock { values.filter { $0.target == target } }
    }

    func save(_ record: PendingEpisodeCommentReceipt) {
        lock.withLock {
            values.removeAll { $0.receiptID == record.receiptID }
            values.append(record)
        }
    }

    func remove(receiptID: UInt64) {
        lock.withLock { values.removeAll { $0.receiptID == receiptID } }
    }

    func removeAll() {
        lock.withLock { values.removeAll() }
    }
}
