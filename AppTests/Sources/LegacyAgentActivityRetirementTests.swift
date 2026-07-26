import Foundation
import XCTest
@testable import Podcastr

final class LegacyAgentActivityRetirementTests: XCTestCase {
    func testDecodesAndRetiresLegacyActivityWithoutResurrection() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        defer { persistence.reset() }
        var legacy = AppState()
        legacy.legacyAgentActivity = [entry(summary: "Private legacy summary")]
        XCTAssertTrue(persistence.write(legacy, revision: 7))

        let loaded = try persistence.load()
        XCTAssertEqual(loaded.legacyAgentActivity, legacy.legacyAgentActivity)

        try persistence.retireLegacyAgentActivitySource(state: loaded)
        XCTAssertTrue(try persistence.load().legacyAgentActivity.isEmpty)

        var staleInMemorySnapshot = loaded
        staleInMemorySnapshot.settings.hasCompletedOnboarding = true
        XCTAssertTrue(persistence.write(staleInMemorySnapshot, revision: 9))
        XCTAssertTrue(
            try persistence.load().legacyAgentActivity.isEmpty,
            "A later native save must not resurrect the retired payload"
        )
    }

    func testRetirementSurvivesRestartAndRemainsIdempotent() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let first = Persistence(fileURL: fileURL)
        var legacy = AppState()
        legacy.legacyAgentActivity = [entry(summary: "Retire once")]
        XCTAssertTrue(first.write(legacy, revision: 3))
        try first.retireLegacyAgentActivitySource(state: try first.load())

        let restarted = Persistence(fileURL: fileURL)
        defer { restarted.reset() }
        let recovered = try restarted.load()
        XCTAssertTrue(recovered.legacyAgentActivity.isEmpty)
        try restarted.retireLegacyAgentActivitySource(state: recovered)
        XCTAssertTrue(restarted.write(recovered, revision: 5))
        XCTAssertTrue(try restarted.load().legacyAgentActivity.isEmpty)
    }

    func testDataExportOmitsRetiredPrivateSummaries() throws {
        var state = AppState()
        state.legacyAgentActivity = [entry(summary: "Do not export this")]

        let payload = DataExport.makePayload(from: state)
        let encoded = try DataExport.encode(payload)
        let json = String(decoding: encoded, as: UTF8.self)

        XCTAssertTrue(payload.state.legacyAgentActivity.isEmpty)
        XCTAssertFalse(json.contains("Do not export this"))
    }

    func testRetiredActivityCannotRegainAWriterOrUserInterface() throws {
        let repositoryRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let sources = repositoryRoot.appendingPathComponent("App/Sources")
        let enumerator = try XCTUnwrap(FileManager.default.enumerator(
            at: sources,
            includingPropertiesForKeys: nil
        ))
        let prohibited = [
            "recordAgentActivity(",
            "undoAgentActivity(",
            "AgentActivityLogView",
            "AgentActivitySheet",
            "activeAgentActivityCount",
        ]
        var violations: [String] = []
        for case let file as URL in enumerator where file.pathExtension == "swift" {
            let contents = try String(contentsOf: file, encoding: .utf8)
            if prohibited.contains(where: contents.contains) {
                violations.append(file.path.replacingOccurrences(
                    of: repositoryRoot.path + "/",
                    with: ""
                ))
            }
        }
        XCTAssertEqual(
            violations,
            [],
            "Retired native Agent activity must remain decode-and-delete only"
        )

        let bubble = try String(
            contentsOf: sources.appendingPathComponent(
                "Features/Agent/AgentChatBubble.swift"
            ),
            encoding: .utf8
        )
        XCTAssertFalse(bubble.contains("onOpenBatch"))
        XCTAssertFalse(bubble.contains("batchUndoneCount"))
        XCTAssertFalse(bubble.contains("batchFirstSummary"))
    }

    private func entry(summary: String) -> LegacyAgentActivityEntry {
        LegacyAgentActivityEntry(
            id: UUID(),
            batchID: UUID(),
            timestamp: Date(timeIntervalSince1970: 1_700_000_000),
            kind: .noteCreated(noteID: UUID()),
            summary: summary,
            undone: false
        )
    }
}
