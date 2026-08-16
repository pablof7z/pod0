import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class EpisodeDiagnosticsAcceptanceTests: XCTestCase {
    func testUnavailableCoreReturnsUnavailablePage() async {
        let page = await CoreEpisodeActivityReader.shared.firstPage(
            for: EpisodeId(high: 1, low: 2),
            from: nil,
            requestedCount: 500
        )
        XCTAssertFalse(page.available)
        XCTAssertTrue(page.items.isEmpty)
        XCTAssertNil(page.nextBeforeSequence)
    }

    func testCorruptAndFutureCoreBootstrapRenderUnavailablePage() async throws {
        try await assertUnavailableAfterCoreMutation { persistence in
            try Data("not-a-sqlite-database".utf8).write(
                to: persistence.sharedCoreStoreURL
            )
        }
        try await assertUnavailableAfterCoreMutation { persistence in
            try WorkflowSQLite.withDatabase(fileURL: persistence.sharedCoreStoreURL) { database in
                try WorkflowSQLite.execute(
                    "PRAGMA user_version=\(sharedStoreSchemaVersion() + 1)",
                    database
                )
            }
        }
    }

    func testFirstPageAndLoadMoreAreBoundedAndSnapshotStable() async throws {
        let fixture = makeFixture()
        defer { dispose(fixture) }
        let facade = try XCTUnwrap(fixture.store.sharedLibrary?.facade)
        for index in 0..<12 {
            facade.dispatch(command: CommandEnvelope(
                commandId: CommandId(high: 91, low: UInt64(index + 1)),
                cancellationId: CancellationId(high: 92, low: UInt64(index + 1)),
                expectedRevision: nil,
                command: .setEpisodeStarred(
                    episodeId: EpisodeId(uuid: fixture.episodeID),
                    starred: index.isMultiple(of: 2)
                )
            ))
        }

        let first = await CoreEpisodeActivityReader.shared.firstPage(
            for: EpisodeId(uuid: fixture.episodeID),
            from: facade,
            requestedCount: 5
        )
        XCTAssertTrue(first.available)
        XCTAssertLessThanOrEqual(first.items.count, 5)
        let snapshot = try XCTUnwrap(first.snapshotThroughSequence)
        XCTAssertNotNil(first.nextBeforeSequence)
        XCTAssertEqual(first.items.map(\.sequence), first.items.map(\.sequence).sorted(by: >))

        facade.dispatch(command: CommandEnvelope(
            commandId: CommandId(high: 91, low: 100),
            cancellationId: CancellationId(high: 92, low: 100),
            expectedRevision: nil,
            command: .setEpisodeStarred(
                episodeId: EpisodeId(uuid: fixture.episodeID),
                starred: true
            )
        ))
        let relaunched = try Pod0Facade.open(
            storePath: fixture.persistence.sharedCoreStoreURL.path
        )

        let loaded = await CoreEpisodeActivityReader.shared.loadMore(
            for: EpisodeId(uuid: fixture.episodeID),
            current: first,
            from: relaunched,
            requestedCount: 5
        )
        XCTAssertEqual(loaded.snapshotThroughSequence, first.snapshotThroughSequence)
        XCTAssertGreaterThan(loaded.items.count, first.items.count)
        XCTAssertLessThanOrEqual(loaded.items.count, 10)
        XCTAssertTrue(loaded.items.allSatisfy { $0.sequence <= snapshot })
    }

    func testProjectedDiagnosticsRemainRedactedAndReaderHasNoMutationAPI() async throws {
        let fixture = makeFixture()
        defer { dispose(fixture) }
        let facade = try XCTUnwrap(fixture.store.sharedLibrary?.facade)
        facade.dispatch(command: CommandEnvelope(
            commandId: CommandId(high: 93, low: 1),
            cancellationId: CancellationId(high: 94, low: 1),
            expectedRevision: nil,
            command: .setEpisodeStarred(
                episodeId: EpisodeId(uuid: fixture.episodeID),
                starred: true
            )
        ))
        let page = await CoreEpisodeActivityReader.shared.firstPage(
            for: EpisodeId(uuid: fixture.episodeID),
            from: facade
        )
        let rendered = page.items.flatMap { entry in
            [entry.title, entry.summary] + entry.details.flatMap { [$0.label, $0.value] }
        }.joined(separator: "|")
        XCTAssertFalse(rendered.contains(Self.secretTitle))
        XCTAssertFalse(rendered.contains(Self.secretURL))

        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let reader = try String(
            contentsOf: root.appendingPathComponent(
                "App/Sources/Core/CoreEpisodeActivityReader.swift"
            ),
            encoding: .utf8
        )
        for forbidden in ["func clear", "func append", "func fabricate"] {
            XCTAssertFalse(reader.contains(forbidden), forbidden)
        }

        let view = try source(
            "App/Sources/Features/EpisodeDetail/EpisodeAuditLogView.swift",
            under: root
        )
        XCTAssertTrue(view.contains("The durable activity journal is unavailable."))
        XCTAssertFalse(view.contains("Button(\"Clear\""))
        let row = try source(
            "App/Sources/Features/EpisodeDetail/EpisodeActivityEntryRow.swift",
            under: root
        )
        for required in [".accessibilityLabel", ".accessibilityValue", ".accessibilityHint"] {
            XCTAssertTrue(row.contains(required), required)
        }
        let sourceRoot = root.appendingPathComponent("App/Sources", isDirectory: true)
        let enumerator = try XCTUnwrap(FileManager.default.enumerator(
            at: sourceRoot,
            includingPropertiesForKeys: nil
        ))
        while let url = enumerator.nextObject() as? URL {
            guard url.pathExtension == "swift" else { continue }
            let production = try String(contentsOf: url, encoding: .utf8)
            XCTAssertFalse(production.contains("EpisodeAuditLogStore"), url.path)
            XCTAssertFalse(production.contains("EpisodeAuditEvent("), url.path)
        }
    }

    private static let secretTitle = "PRIVATE-DIAGNOSTIC-TITLE"
    private static let secretURL = "https://secret.invalid/private-token.mp3"

    private func makeFixture() -> (persistence: Persistence, store: AppStateStore, episodeID: UUID) {
        let persistence = Persistence(fileURL: AppStateTestSupport.uniqueTempFileURL())
        let podcastID = UUID()
        let episodeID = UUID()
        var state = AppState()
        state.podcasts = [Podcast(
            id: podcastID,
            feedURL: URL(string: "https://example.test/feed.xml")!,
            title: "Diagnostics"
        )]
        state.subscriptions = [PodcastSubscription(podcastID: podcastID)]
        state.episodes = [Episode(
            id: episodeID,
            podcastID: podcastID,
            guid: "diagnostics",
            title: Self.secretTitle,
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: Self.secretURL)!
        )]
        XCTAssertTrue(persistence.write(state, revision: 1))
        let preparation = AppStateStartupPreparer.prepare(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([])
        )
        if case .authoritativeUnavailable(let reason, let stage)? = preparation.bootstrap {
            XCTFail("shared bootstrap failed at \(stage.rawValue): \(reason)")
        }
        let store = AppStateStore(
            preparedStartup: preparation,
            persistence: persistence,
            productSignals: DiscardingProductSignalSink.shared,
            startSubscriptionRefresh: false
        )
        XCTAssertNil(store.sharedLibraryUnavailableReason)
        XCTAssertNotNil(store.sharedLibrary)
        return (persistence, store, episodeID)
    }

    private func dispose(
        _ fixture: (persistence: Persistence, store: AppStateStore, episodeID: UUID)
    ) {
        guard fixture.store.sharedLibrary != nil else { return }
        fixture.store.sharedLibrary?.shutdown()
        fixture.persistence.reset()
    }

    private func assertUnavailableAfterCoreMutation(
        _ mutate: (Persistence) throws -> Void
    ) async throws {
        let fixture = makeFixture()
        fixture.store.sharedLibrary?.shutdown()
        defer { fixture.persistence.reset() }
        try mutate(fixture.persistence)
        let blocked = AppStateStore(
            persistence: fixture.persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertNil(blocked.sharedLibrary)
        let page = await CoreEpisodeActivityReader.shared.firstPage(
            for: EpisodeId(uuid: fixture.episodeID),
            from: blocked.sharedLibrary?.facade
        )
        XCTAssertFalse(page.available)
        XCTAssertTrue(page.items.isEmpty)
    }

    private func source(_ path: String, under root: URL) throws -> String {
        try String(contentsOf: root.appendingPathComponent(path), encoding: .utf8)
    }
}
