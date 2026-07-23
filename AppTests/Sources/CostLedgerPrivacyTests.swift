import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class CostLedgerPrivacyTests: XCTestCase {
    func testNewRecordsPersistOnlyMeteringFacts() throws {
        let fixture = makeFixture()
        defer { fixture.dispose() }
        let ledger = CostLedger(fileURL: fixture.fileURL, now: { fixture.now })

        ledger.logOllama(
            feature: CostFeature.agentChat,
            model: "local-model",
            promptTokens: 12,
            completionTokens: 4,
            latencyMs: 50
        )

        let record = try XCTUnwrap(ledger.records.first)
        XCTAssertNil(record.requestPayloadJSON)
        XCTAssertNil(record.responseContentPreview)
        let persisted = try String(contentsOf: fixture.fileURL, encoding: .utf8)
        XCTAssertFalse(persisted.contains("requestPayloadJSON"))
        XCTAssertFalse(persisted.contains("responseContentPreview"))
    }

    func testLoadSanitizesContentAndAppliesAgeAndCountRetention() throws {
        let fixture = makeFixture()
        defer { fixture.dispose() }
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        var records = (0..<505).map { index in
            var record = makeRecord(
                at: fixture.now.addingTimeInterval(-Double(index))
            )
            record.requestPayloadJSON = #"{"token":"SECRET"}"#
            record.responseContentPreview = "PRIVATE RESPONSE"
            return record
        }
        records.append(makeRecord(
            at: fixture.now.addingTimeInterval(-100 * 86_400)
        ))
        try FileManager.default.createDirectory(
            at: fixture.directoryURL,
            withIntermediateDirectories: true
        )
        try encoder.encode(records).write(to: fixture.fileURL)

        let ledger = CostLedger(fileURL: fixture.fileURL, now: { fixture.now })

        XCTAssertEqual(ledger.records.count, CostLedger.maximumRecordCount)
        XCTAssertTrue(ledger.records.allSatisfy {
            $0.requestPayloadJSON == nil && $0.responseContentPreview == nil
        })
        XCTAssertTrue(zip(ledger.records, ledger.records.dropFirst()).allSatisfy {
            $0.0.at >= $0.1.at
        })
        let persisted = try String(contentsOf: fixture.fileURL, encoding: .utf8)
        XCTAssertFalse(persisted.contains("SECRET"))
        XCTAssertFalse(persisted.contains("PRIVATE RESPONSE"))
    }

    func testCorruptLedgerIsPreservedUntilExplicitReset() throws {
        let fixture = makeFixture()
        defer { fixture.dispose() }
        try FileManager.default.createDirectory(
            at: fixture.directoryURL,
            withIntermediateDirectories: true
        )
        let corrupt = Data("SECRET-corrupt-ledger".utf8)
        try corrupt.write(to: fixture.fileURL)
        let ledger = CostLedger(fileURL: fixture.fileURL, now: { fixture.now })

        XCTAssertEqual(ledger.persistenceStatus, .unavailable)
        ledger.logOllama(
            feature: CostFeature.agentChat,
            model: "ignored",
            promptTokens: 1,
            completionTokens: 1,
            latencyMs: 1
        )
        XCTAssertEqual(try Data(contentsOf: fixture.fileURL), corrupt)

        ledger.clear()
        XCTAssertEqual(ledger.persistenceStatus, .ready)
        XCTAssertEqual(ledger.records, [])
        XCTAssertEqual(
            try JSONDecoder().decode([UsageRecord].self, from: Data(
                contentsOf: fixture.fileURL
            )),
            []
        )
    }

    func testProviderCallSitesCannotFeedContentIntoLedger() throws {
        let sources = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("App/Sources")
        let relativePaths = [
            "Features/Agent/AgentOpenRouterClient.swift",
            "Features/Agent/AgentOllamaClient.swift",
            "Knowledge/EmbeddingsClient.swift",
            "Knowledge/OllamaEmbeddingsClient.swift",
            "Knowledge/UtilityLLMClient.swift",
        ]
        for path in relativePaths {
            let source = try String(
                contentsOf: sources.appendingPathComponent(path),
                encoding: .utf8
            )
            XCTAssertFalse(source.contains("requestPayloadJSON"), path)
            XCTAssertFalse(source.contains("responseContentPreview"), path)
        }
    }

    private func makeRecord(at: Date) -> UsageRecord {
        UsageRecord(
            id: UUID(),
            at: at,
            feature: CostFeature.agentChat,
            model: "fixture-model",
            promptTokens: 10,
            completionTokens: 5,
            cachedTokens: 2,
            reasoningTokens: 1,
            costUSD: 0.01,
            latencyMs: 100
        )
    }

    private func makeFixture() -> LedgerFixture {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("CostLedgerTests-\(UUID().uuidString)")
        return LedgerFixture(
            directoryURL: directory,
            fileURL: directory.appendingPathComponent("ledger.json"),
            now: Date(timeIntervalSince1970: 1_800_000_000)
        )
    }
}

private struct LedgerFixture {
    let directoryURL: URL
    let fileURL: URL
    let now: Date

    func dispose() {
        try? FileManager.default.removeItem(at: directoryURL)
    }
}
