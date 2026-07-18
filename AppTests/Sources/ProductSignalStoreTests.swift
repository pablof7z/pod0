import XCTest
@testable import Podcastr

final class ProductSignalStoreTests: XCTestCase {
    func testRecordIsVersionedDeduplicatedAndDurable() async {
        let url = ProductSignalTestSupport.uniqueFileURL()
        defer { ProductSignalTestSupport.dispose(url) }
        let store = ProductSignalStore(fileURL: url)
        let observation = ProductSignalObservation(
            signalID: UUID(uuidString: "11111111-1111-1111-1111-111111111111")!,
            occurredAt: Date(timeIntervalSince1970: 1_700_000_000),
            name: .playStarted,
            outcome: .succeeded,
            domainRevision: 7
        )

        await store.record(observation)
        await store.record(observation)

        let reopened = ProductSignalStore(fileURL: url)
        let snapshot = await reopened.snapshot()
        XCTAssertEqual(snapshot.signals.count, 1)
        XCTAssertEqual(snapshot.signals[0].schemaVersion, 1)
        XCTAssertEqual(snapshot.signals[0].domainRevision, 7)
    }

    func testOptOutDeletesSignalsRotatesIdentityAndFailsClosedForCollection() async {
        let url = ProductSignalTestSupport.uniqueFileURL()
        defer { ProductSignalTestSupport.dispose(url) }
        let store = ProductSignalStore(fileURL: url)
        await store.record(.init(name: .appLaunch, outcome: .started))
        let originalID = await store.snapshot().anonymousInstallID

        await store.setEnabled(false)
        await store.record(.init(name: .noteCreated, outcome: .created))
        let disabled = await store.snapshot()

        XCTAssertFalse(disabled.isEnabled)
        XCTAssertTrue(disabled.signals.isEmpty)
        XCTAssertNotEqual(disabled.anonymousInstallID, originalID)

        await store.setEnabled(true)
        await store.record(.init(name: .noteCreated, outcome: .created))
        let enabled = await store.snapshot()
        XCTAssertEqual(enabled.signals.count, 1)
    }

    func testDeleteAllClearsSignalsAndRotatesIdentity() async {
        let url = ProductSignalTestSupport.uniqueFileURL()
        defer { ProductSignalTestSupport.dispose(url) }
        let store = ProductSignalStore(fileURL: url)
        await store.record(.init(name: .clipCreated, outcome: .created))
        let originalID = await store.snapshot().anonymousInstallID

        await store.deleteAll()
        let deleted = await store.snapshot()

        XCTAssertTrue(deleted.isEnabled)
        XCTAssertTrue(deleted.signals.isEmpty)
        XCTAssertNotEqual(deleted.anonymousInstallID, originalID)
    }

    func testSessionMarkerDetectsPriorUncleanTermination() async {
        let url = ProductSignalTestSupport.uniqueFileURL()
        defer { ProductSignalTestSupport.dispose(url) }
        let firstProcess = ProductSignalStore(fileURL: url)
        let firstDate = Date(timeIntervalSince1970: 1_700_000_000)
        await firstProcess.setSessionActive(true, now: firstDate)

        let nextProcess = ProductSignalStore(fileURL: url)
        await nextProcess.setSessionActive(true, now: firstDate.addingTimeInterval(60))
        let names = await nextProcess.snapshot().signals.map(\.name)

        XCTAssertEqual(names.filter { $0 == .appLaunch }.count, 2)
        XCTAssertEqual(names.filter { $0 == .uncleanTermination }.count, 1)
    }

    func testCleanSessionDoesNotProduceTerminationSignal() async {
        let url = ProductSignalTestSupport.uniqueFileURL()
        defer { ProductSignalTestSupport.dispose(url) }
        let firstProcess = ProductSignalStore(fileURL: url)
        await firstProcess.setSessionActive(true)
        await firstProcess.setSessionActive(false)

        let nextProcess = ProductSignalStore(fileURL: url)
        await nextProcess.setSessionActive(true)
        let names = await nextProcess.snapshot().signals.map(\.name)

        XCTAssertEqual(names.filter { $0 == .appLaunch }.count, 2)
        XCTAssertFalse(names.contains(.uncleanTermination))
    }

    func testPersistenceFailureDoesNotRejectInMemorySignal() async throws {
        let parent = ProductSignalTestSupport.uniqueFileURL().deletingLastPathComponent()
        defer { try? FileManager.default.removeItem(at: parent) }
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
        let directoryAsFile = parent.appendingPathComponent("archive.json", isDirectory: true)
        try FileManager.default.createDirectory(at: directoryAsFile, withIntermediateDirectories: true)
        let store = ProductSignalStore(fileURL: directoryAsFile)

        await store.record(.init(name: .playStarted, outcome: .succeeded))

        let snapshot = await store.snapshot()
        XCTAssertEqual(snapshot.signals.count, 1)
    }

    func testExportContainsOnlyTypedContentFreeFields() async throws {
        let url = ProductSignalTestSupport.uniqueFileURL()
        defer { ProductSignalTestSupport.dispose(url) }
        let store = ProductSignalStore(fileURL: url)
        await store.record(.init(
            occurredAt: Date(timeIntervalSince1970: 1_700_000_000),
            name: .recallGrounded,
            outcome: .grounded,
            latencyBucket: .milliseconds250To749,
            errorClass: .network,
            domainRevision: 9
        ))

        let exported = await store.exportData()
        let data = try XCTUnwrap(exported)
        let text = try XCTUnwrap(String(data: data, encoding: .utf8))
        XCTAssertFalse(text.contains("private query about a named guest"))
        for forbiddenKey in ["query", "transcript", "title", "note", "clip", "url", "path", "credential"] {
            XCTAssertFalse(text.contains("\"\(forbiddenKey)\":"), forbiddenKey)
        }
        XCTAssertTrue(text.contains("\"schemaVersion\":1"))
        XCTAssertTrue(text.contains("\"recallGrounded\""))
    }

    func testLatencyBucketsUsePredeclaredBoundaries() {
        XCTAssertEqual(ProductSignalLatencyBucket.bucket(.milliseconds(249)), .under250Milliseconds)
        XCTAssertEqual(ProductSignalLatencyBucket.bucket(.milliseconds(250)), .milliseconds250To749)
        XCTAssertEqual(ProductSignalLatencyBucket.bucket(.milliseconds(750)), .milliseconds750To1999)
        XCTAssertEqual(ProductSignalLatencyBucket.bucket(.seconds(2)), .seconds2To4)
        XCTAssertEqual(ProductSignalLatencyBucket.bucket(.seconds(5)), .seconds5Plus)
    }

    func testReportCountsDistinctDaysAndActivation() {
        let installID = UUID()
        let dayOne = Date(timeIntervalSince1970: 1_700_000_000)
        let dayTwo = dayOne.addingTimeInterval(86_400)
        let signals = [
            ProductSignal(observation: .init(occurredAt: dayOne, name: .firstSubscription, outcome: .created), anonymousInstallID: installID),
            ProductSignal(observation: .init(occurredAt: dayTwo, name: .playStarted, outcome: .succeeded), anonymousInstallID: installID),
        ]

        let report = ProductSignalReport(signals: signals, generatedAt: dayTwo)

        XCTAssertEqual(report.signalCount, 2)
        XCTAssertEqual(report.distinctActiveDays, 2)
        XCTAssertEqual(report.activatedAt, dayOne)
        XCTAssertEqual(report.counts.map(\.count), [1, 1])
    }
}
