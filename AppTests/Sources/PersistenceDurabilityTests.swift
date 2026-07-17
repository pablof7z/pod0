import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class PersistenceDurabilityTests: XCTestCase {
    func testOutOfOrderRevisionCannotOverwriteNewerSQLiteSnapshot() throws {
        let url = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: url) }
        let persistence = Persistence(fileURL: url)
        var newer = AppState()
        newer.settings.hasCompletedOnboarding = true
        var stale = newer
        stale.settings.hasCompletedOnboarding = false

        XCTAssertTrue(persistence.write(newer, revision: 2))
        XCTAssertTrue(persistence.write(stale, revision: 1))

        let loaded = try Persistence(fileURL: url).load()
        XCTAssertTrue(loaded.settings.hasCompletedOnboarding)
        XCTAssertEqual(loaded.persistenceGeneration, 2)
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path))
    }

    func testBackgroundWriterSerializesARealEnqueueReversal() async throws {
        let url = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: url) }
        let gate = PersistenceFaultGate(blockOnceAt: .afterMetadataStatement)
        let persistence = Persistence(fileURL: url, faultInjector: gate.inject)
        let writer = PersistenceBackgroundWriter()
        var stale = AppState()
        stale.settings.hasCompletedOnboarding = false
        var newest = stale
        newest.settings.hasCompletedOnboarding = true

        await writer.enqueue(revision: 1, state: stale, jobs: [], persistence: persistence)
        while !gate.hasEntered { await Task.yield() }
        await writer.enqueue(revision: 2, state: newest, jobs: [], persistence: persistence)
        gate.release()

        let written = await writer.waitUntilWritten(2)
        XCTAssertTrue(written)
        let loaded = try Persistence(fileURL: url).load()
        XCTAssertTrue(loaded.settings.hasCompletedOnboarding)
        XCTAssertEqual(loaded.persistenceGeneration, 2)
    }

    func testSaveAllocatedRevisionsSurviveReversedProductionEnqueueOrder() async throws {
        let url = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: url) }
        let gate = RevisionEnqueueGate(blockingRevision: 1)
        let persistence = Persistence(
            fileURL: url,
            writeMode: .background,
            beforeBackgroundEnqueue: { revision in await gate.waitIfBlocked(revision) }
        )
        var first = AppState()
        first.settings.hasCompletedOnboarding = false
        var second = first
        second.settings.hasCompletedOnboarding = true

        let firstRevision = persistence.save(first)
        XCTAssertEqual(firstRevision, 1)
        while !(await gate.hasEntered) { await Task.yield() }
        let secondRevision = persistence.save(second)
        XCTAssertEqual(secondRevision, 2)
        let wroteSecond = await persistence.waitUntilWritten(secondRevision)
        XCTAssertTrue(wroteSecond)
        await gate.release()
        try await Task.sleep(for: .milliseconds(20))

        let loaded = try Persistence(fileURL: url).load()
        XCTAssertTrue(loaded.settings.hasCompletedOnboarding)
        XCTAssertEqual(loaded.persistenceGeneration, secondRevision)
    }

    func testBackgroundFlushAwaitsAuthoritativeRevision() async throws {
        let url = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: url) }
        let persistence = Persistence(fileURL: url, writeMode: .background)
        var state = AppState()
        state.settings.hasCompletedOnboarding = true
        let episode = makeEpisode(guid: "flush")
        state.episodes = [episode]

        let flushed = await persistence.flush(state)
        XCTAssertTrue(flushed)
        let loaded = try Persistence(fileURL: url).load()
        XCTAssertTrue(loaded.settings.hasCompletedOnboarding)
        XCTAssertEqual(loaded.episodes.map(\.guid), ["flush"])
    }

    func testFailedAtomicOccurrenceIsCarriedIntoTheNextSnapshot() async throws {
        let url = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: url) }
        let gate = OneShotPersistenceFaultGate(throwOnceAt: .beforeCommit)
        let persistence = Persistence(
            fileURL: url,
            writeMode: .background,
            faultInjector: gate.inject
        )
        let episode = makeEpisode(guid: "discovered")
        var discovered = AppState()
        discovered.episodes = [episode]
        let occurrence = DesiredJob(
            idempotencyKey: "discovery:carried",
            kind: .feedDiscovery,
            subjectID: episode.podcastID,
            inputVersion: "batch-v1",
            occurrenceID: "discovery:carried",
            resourceClass: .planning
        )

        let failedRevision = persistence.save(discovered, ensuring: [occurrence])
        let firstWriteSucceeded = await persistence.waitUntilWritten(failedRevision)
        XCTAssertFalse(firstWriteSucceeded)

        var later = discovered
        later.settings.hasCompletedOnboarding = true
        let recoveredRevision = persistence.save(later)
        let secondWriteSucceeded = await persistence.waitUntilWritten(recoveredRevision)
        XCTAssertTrue(secondWriteSucceeded)

        let loaded = try Persistence(fileURL: url).load()
        XCTAssertEqual(loaded.episodes.map(\.id), [episode.id])
        XCTAssertTrue(loaded.settings.hasCompletedOnboarding)
        let jobs = JobStore(fileURL: Persistence.episodeStoreURL(for: url))
        XCTAssertEqual(
            try jobs.job(idempotencyKey: occurrence.idempotencyKey)?.state,
            .pending
        )
    }

    func testFaultsBeforeCommitExposeOnlyPreviousRevision() throws {
        let points: [EpisodeSQLiteFaultPoint] = [
            .afterEpisodeStatement(0), .afterMetadataStatement,
            .afterJobStatement(0), .beforeCommit,
        ]
        for point in points {
            let fixture = try makeRevisionFixture()
            defer { AppStateTestSupport.disposeIsolatedStore(at: fixture.url) }
            let gate = PersistenceFaultGate(throwAt: point)
            let faulted = Persistence(fileURL: fixture.url, faultInjector: gate.inject)

            XCTAssertFalse(faulted.write(
                fixture.next, revision: 2, ensuring: [fixture.job]
            ), "Expected injected fault at \(point)")

            let loaded = try Persistence(fileURL: fixture.url).load()
            XCTAssertEqual(loaded.persistenceGeneration, 1)
            XCTAssertFalse(loaded.settings.hasCompletedOnboarding)
            XCTAssertEqual(loaded.episodes.map(\.guid), ["old"])
            let jobs = JobStore(fileURL: Persistence.episodeStoreURL(for: fixture.url))
            XCTAssertNil(try jobs.job(idempotencyKey: fixture.job.idempotencyKey))
        }
    }

    func testFaultAfterCommitExposesCompleteNextRevision() throws {
        let fixture = try makeRevisionFixture()
        defer { AppStateTestSupport.disposeIsolatedStore(at: fixture.url) }
        let gate = PersistenceFaultGate(throwAt: .afterCommit)
        let faulted = Persistence(fileURL: fixture.url, faultInjector: gate.inject)

        XCTAssertFalse(faulted.write(fixture.next, revision: 2, ensuring: [fixture.job]))

        let loaded = try Persistence(fileURL: fixture.url).load()
        XCTAssertEqual(loaded.persistenceGeneration, 2)
        XCTAssertTrue(loaded.settings.hasCompletedOnboarding)
        XCTAssertEqual(loaded.episodes.map(\.guid), ["new-a", "new-b"])
        let jobs = JobStore(fileURL: Persistence.episodeStoreURL(for: fixture.url))
        XCTAssertEqual(try jobs.job(idempotencyKey: fixture.job.idempotencyKey)?.state, .pending)
    }

    func testLegacyJSONOnlyMigrationIsIdempotentAndBecomesSQLiteAuthoritative() throws {
        let url = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: url) }
        var legacy = AppState()
        legacy.settings.hasCompletedOnboarding = true
        legacy.episodes = [makeEpisode(guid: "json")]
        try encode(legacy).write(to: url, options: .atomic)

        let first = try Persistence(fileURL: url).load()
        let second = try Persistence(fileURL: url).load()

        XCTAssertEqual(first.persistenceGeneration, second.persistenceGeneration)
        XCTAssertEqual(first.episodes, second.episodes)
        XCTAssertEqual(first.settings, second.settings)
        XCTAssertEqual(second.episodes.map(\.guid), ["json"])
        XCTAssertTrue(second.settings.hasCompletedOnboarding)
    }

    func testMismatchedLegacySourcesUseJSONMetadataAndSQLiteEpisodes() throws {
        let url = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: url) }
        var json = AppState()
        json.settings.hasCompletedOnboarding = true
        json.episodes = [makeEpisode(guid: "json-episode")]
        try encode(json).write(to: url, options: .atomic)
        let episodeStore = EpisodeSQLiteStore(fileURL: Persistence.episodeStoreURL(for: url))
        try episodeStore.replaceAll([makeEpisode(guid: "sqlite-episode")])

        let loaded = try Persistence(fileURL: url).load()

        XCTAssertTrue(loaded.settings.hasCompletedOnboarding)
        XCTAssertEqual(loaded.episodes.map(\.guid), ["sqlite-episode"])
        let reloaded = try Persistence(fileURL: url).load()
        XCTAssertEqual(reloaded.persistenceGeneration, loaded.persistenceGeneration)
        XCTAssertEqual(reloaded.episodes, loaded.episodes)
        XCTAssertEqual(reloaded.settings, loaded.settings)
    }

    func testSQLiteOnlyMigrationPreservesEpisodesAndBecomesAuthoritative() throws {
        let url = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: url) }
        let sidecar = EpisodeSQLiteStore(fileURL: Persistence.episodeStoreURL(for: url))
        try sidecar.replaceAll([makeEpisode(guid: "sqlite-only")])

        let migrated = try Persistence(fileURL: url).load()
        let reloaded = try Persistence(fileURL: url).load()

        XCTAssertEqual(migrated.episodes.map(\.guid), ["sqlite-only"])
        XCTAssertEqual(reloaded.episodes, migrated.episodes)
        XCTAssertEqual(reloaded.persistenceGeneration, migrated.persistenceGeneration)
    }

    func testMatchingLegacySourcesMigrateIdempotently() throws {
        let url = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: url) }
        var legacy = AppState()
        legacy.settings.hasCompletedOnboarding = true
        let episode = makeEpisode(guid: "matching")
        legacy.episodes = [episode]
        try encode(legacy).write(to: url, options: .atomic)
        try EpisodeSQLiteStore(
            fileURL: Persistence.episodeStoreURL(for: url)
        ).replaceAll([episode])

        let migrated = try Persistence(fileURL: url).load()
        let reloaded = try Persistence(fileURL: url).load()

        XCTAssertEqual(migrated.episodes.map(\.id), [episode.id])
        XCTAssertEqual(migrated.episodes.map(\.guid), [episode.guid])
        XCTAssertTrue(migrated.settings.hasCompletedOnboarding)
        XCTAssertEqual(reloaded.episodes, migrated.episodes)
        XCTAssertEqual(reloaded.settings, migrated.settings)
        XCTAssertEqual(reloaded.persistenceGeneration, migrated.persistenceGeneration)
    }

    func testInterruptedLegacyMigrationCanRestart() throws {
        let url = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: url) }
        var legacy = AppState()
        legacy.episodes = [makeEpisode(guid: "restart")]
        try encode(legacy).write(to: url, options: .atomic)
        let gate = PersistenceFaultGate(throwAt: .beforeCommit)

        XCTAssertThrowsError(try Persistence(fileURL: url, faultInjector: gate.inject).load())
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))

        let recovered = try Persistence(fileURL: url).load()
        XCTAssertEqual(recovered.episodes.map(\.guid), ["restart"])
    }

    private func makeRevisionFixture() throws -> (
        url: URL, next: AppState, job: DesiredJob
    ) {
        let url = AppStateTestSupport.uniqueTempFileURL()
        var previous = AppState()
        previous.episodes = [makeEpisode(guid: "old")]
        XCTAssertTrue(Persistence(fileURL: url).write(previous, revision: 1))
        var next = previous
        next.settings.hasCompletedOnboarding = true
        next.episodes = [makeEpisode(guid: "new-a"), makeEpisode(guid: "new-b")]
        let job = DesiredJob(
            idempotencyKey: "metadata:fault:v2", kind: .metadataIndex,
            subjectID: next.episodes[0].id, inputVersion: "v2",
            resourceClass: .embedding
        )
        return (url, next, job)
    }

    private func makeEpisode(guid: String) -> Episode {
        Episode(
            podcastID: UUID(), guid: guid, title: guid, pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/\(guid).mp3")!
        )
    }

    private func encode(_ state: AppState) throws -> Data {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(state)
    }
}

private enum InjectedPersistenceFault: Error { case expected }

private actor RevisionEnqueueGate {
    let blockingRevision: UInt64
    private(set) var hasEntered = false
    private var continuation: CheckedContinuation<Void, Never>?

    init(blockingRevision: UInt64) {
        self.blockingRevision = blockingRevision
    }

    func waitIfBlocked(_ revision: UInt64) async {
        guard revision == blockingRevision else { return }
        hasEntered = true
        await withCheckedContinuation { continuation = $0 }
    }

    func release() {
        continuation?.resume()
        continuation = nil
    }
}

private final class PersistenceFaultGate: @unchecked Sendable {
    private let lock = NSLock()
    private let semaphore = DispatchSemaphore(value: 0)
    private let throwPoint: EpisodeSQLiteFaultPoint?
    private let blockPoint: EpisodeSQLiteFaultPoint?
    private var entered = false
    private var didBlock = false

    init(throwAt point: EpisodeSQLiteFaultPoint) {
        throwPoint = point
        blockPoint = nil
    }

    init(blockOnceAt point: EpisodeSQLiteFaultPoint) {
        throwPoint = nil
        blockPoint = point
    }

    var hasEntered: Bool {
        lock.withLock { entered }
    }

    func inject(_ point: EpisodeSQLiteFaultPoint) throws {
        if point == throwPoint { throw InjectedPersistenceFault.expected }
        let shouldBlock = lock.withLock { () -> Bool in
            guard point == blockPoint, !didBlock else { return false }
            didBlock = true
            entered = true
            return true
        }
        if shouldBlock { semaphore.wait() }
    }

    func release() { semaphore.signal() }
}

private final class OneShotPersistenceFaultGate: @unchecked Sendable {
    private let lock = NSLock()
    private let point: EpisodeSQLiteFaultPoint
    private var hasThrown = false

    init(throwOnceAt point: EpisodeSQLiteFaultPoint) {
        self.point = point
    }

    func inject(_ candidate: EpisodeSQLiteFaultPoint) throws {
        let shouldThrow = lock.withLock { () -> Bool in
            guard candidate == point, !hasThrown else { return false }
            hasThrown = true
            return true
        }
        if shouldThrow { throw InjectedPersistenceFault.expected }
    }
}
