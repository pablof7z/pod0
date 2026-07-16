import Foundation
import XCTest
@testable import Podcastr

final class NMPInstalledStateMigrationTests: XCTestCase {
    func testPreparationPreservesProductPolicyAndQuarantinesProtocolAuthority() throws {
        let fixture = legacyFixture()
        let (migration, layout, root) = try makeMigration()
        defer { try? FileManager.default.removeItem(at: root) }
        let now = Date(timeIntervalSince1970: 1_789_000_000)

        let result = try migration.prepareIfNeeded(
            state: fixture,
            expectedPublicKeys: [.human: String(repeating: "a", count: 64)],
            now: now
        )

        XCTAssertTrue(result.didPrepare)
        XCTAssertEqual(result.activeState.friends, fixture.friends)
        XCTAssertEqual(result.activeState.nostrAllowedPubkeys, fixture.nostrAllowedPubkeys)
        XCTAssertEqual(result.activeState.nostrBlockedPubkeys, fixture.nostrBlockedPubkeys)
        XCTAssertEqual(result.activeState.agentActivity.count, fixture.agentActivity.count)
        XCTAssertTrue(result.activeState.nostrPendingApprovals.isEmpty)
        XCTAssertTrue(result.activeState.pendingFriendMessages.isEmpty)
        XCTAssertTrue(result.activeState.nostrConversations.isEmpty)
        XCTAssertTrue(result.activeState.nostrProfileCache.isEmpty)
        XCTAssertTrue(result.activeState.nostrRespondedEventIDs.isEmpty)
        XCTAssertNil(result.activeState.nostrSinceCursor)
        XCTAssertTrue(result.activeState.settings.nostrPublicRelays.isEmpty)
        XCTAssertTrue(result.record.failClosedLegacyIngress)
        XCTAssertEqual(result.record.pinnedNMPCommit, Pod0NMPBuild.testedRevision)

        let archive = try decodeArchive(at: layout.quarantineArchiveURL)
        XCTAssertEqual(archive.conversations.first?.turns.first?.rawEventJSON, "{\"secret\":\"not-a-key\"}")
        XCTAssertEqual(archive.conversationsDeleteAt, .remoteAgentConversations)
        XCTAssertEqual(archive.profileCacheDeleteAt, .profilesAndTrustSurfaces)
        XCTAssertEqual(archive.discoveredRelayURLsDeleteAt, .podcastLifecycle)
    }

    func testPreparationAndPhaseTransitionsAreIdempotent() throws {
        let fixture = legacyFixture()
        let (migration, _, root) = try makeMigration()
        defer { try? FileManager.default.removeItem(at: root) }

        let first = try migration.prepareIfNeeded(state: fixture, expectedPublicKeys: [:])
        let second = try migration.prepareIfNeeded(state: fixture, expectedPublicKeys: [:])
        try migration.verifySourceIsIdempotent(fixture)

        XCTAssertTrue(first.didPrepare)
        XCTAssertFalse(second.didPrepare)
        XCTAssertEqual(first.record, second.record)
        XCTAssertTrue(second.activeState.nostrConversations.isEmpty)
        let qualified = try XCTUnwrap(try migration.advance(to: .nmpQualified))
        let repeated = try XCTUnwrap(try migration.advance(to: .nmpQualified))
        XCTAssertEqual(qualified, repeated)
        XCTAssertThrowsError(try migration.advance(to: .prepared))
    }

    func testRecordAndDefaultExportContainNoSecretOrRawLegacyEvent() throws {
        var fixture = legacyFixture()
        fixture.settings.legacyOpenRouterAPIKey = "super-secret-api-key"
        let (migration, layout, root) = try makeMigration()
        defer { try? FileManager.default.removeItem(at: root) }
        _ = try migration.prepareIfNeeded(state: fixture, expectedPublicKeys: [:])

        let recordJSON = try String(contentsOf: layout.migrationRecordURL, encoding: .utf8)
        XCTAssertFalse(recordJSON.contains("super-secret-api-key"))
        XCTAssertFalse(recordJSON.contains("not-a-key"))

        let exportData = try DataExport.encode(DataExport.makePayload(from: fixture))
        let exportJSON = String(decoding: exportData, as: UTF8.self)
        XCTAssertFalse(exportJSON.contains("not-a-key"))
        XCTAssertFalse(exportJSON.contains("future-event"))
        XCTAssertTrue(exportJSON.contains("quarantined locally and excluded by default"))
    }

    func testRollbackBeforeCutoverResetsOnlyStagedStoreArtifacts() throws {
        let fixture = legacyFixture()
        let (migration, layout, root) = try makeMigration()
        defer { try? FileManager.default.removeItem(at: root) }
        _ = try migration.prepareIfNeeded(state: fixture, expectedPublicKeys: [:])
        var resetCalled = false

        try migration.rollbackBeforeCutover { resetCalled = true }

        XCTAssertTrue(resetCalled)
        XCTAssertFalse(FileManager.default.fileExists(atPath: layout.migrationRecordURL.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: layout.quarantineArchiveURL.path))
    }

    private func makeMigration() throws -> (NMPInstalledStateMigration, Pod0NMPStoreLayout, URL) {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("pod0-nmp-migration-\(UUID().uuidString)", isDirectory: true)
        let layout = Pod0NMPStoreLayout(rootDirectory: root)
        return (NMPInstalledStateMigration(layout: layout), layout, root)
    }

    private func decodeArchive(at url: URL) throws -> LegacyNostrQuarantineV1 {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(LegacyNostrQuarantineV1.self, from: Data(contentsOf: url))
    }

    private func legacyFixture() -> AppState {
        var state = AppState()
        state.friends = [Friend(displayName: "Alice", identifier: "alice")]
        state.nostrAllowedPubkeys = ["allowed"]
        state.nostrBlockedPubkeys = ["blocked"]
        state.nostrPendingApprovals = [NostrPendingApproval(pubkeyHex: "pending", content: "hello")]
        state.nostrConversations = [NostrConversationRecord(
            rootEventID: "root",
            counterpartyPubkey: "peer",
            firstSeen: .distantPast,
            lastTouched: .distantFuture,
            turns: [NostrConversationTurn(
                eventID: "future-event",
                direction: .incoming,
                pubkey: "peer",
                createdAt: .distantFuture,
                content: "run a tool",
                rawEventJSON: "{\"secret\":\"not-a-key\"}"
            )]
        )]
        state.nostrProfileCache["peer"] = NostrProfileMetadata(
            pubkey: "peer",
            name: "Unverified",
            displayName: nil,
            about: nil,
            picture: nil,
            nip05: nil,
            fetchedFromCreatedAt: Int.max
        )
        state.nostrRespondedEventIDs = ["future-event"]
        state.nostrSinceCursor = Int.max
        state.settings.nostrPublicRelays = ["wss://discovered.example"]
        return state
    }
}

