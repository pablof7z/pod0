import Foundation
import Pod0Core
import XCTest

final class Pod0CoreBindingTests: XCTestCase {
    func testSwiftAndKotlinSchemaCompatibilityFixture() throws {
        let fixtureURL = try XCTUnwrap(
            Bundle(for: Self.self).url(
                forResource: "schema-status-v1",
                withExtension: "properties"
            )
        )
        let fixture = try decodeProperties(at: fixtureURL)

        XCTAssertEqual(fixture["fixture_version"], "1")
        XCTAssertEqual(fixture["schema_component"], "kernel")
        XCTAssertEqual(UInt32(fixture["stored_version"] ?? ""), 2)
        XCTAssertEqual(UInt32(fixture["supported_min"] ?? ""), 0)
        XCTAssertEqual(UInt32(fixture["supported_max"] ?? ""), 7)
        XCTAssertEqual(fixture["access_mode"], "migration_only")
        XCTAssertEqual(fixture["migration_state"], "required")
        XCTAssertEqual(UInt32(fixture["target_version"] ?? ""), 7)
        XCTAssertEqual(UInt64(fixture["store_id_high"] ?? ""), 10)
        XCTAssertEqual(UInt64(fixture["store_id_low"] ?? ""), 11)
        XCTAssertEqual(UInt64(fixture["command_id_high"] ?? ""), 1)
        XCTAssertEqual(UInt64(fixture["command_id_low"] ?? ""), 2)
        XCTAssertEqual(UInt64(fixture["state_revision"] ?? ""), 42)
        XCTAssertEqual(fixture["operation_stage"], "failed")
        XCTAssertEqual(fixture["error_kind"], "unsupported")
        XCTAssertEqual(UInt32(fixture["error_wire_code"] ?? ""), 9_001)
        XCTAssertEqual(fixture["optional_safe_detail"], "null")
    }

    func testGeneratedFacadeRoundTripsCommandsProjectionsAndSubscriptionLifecycle() throws {
        let facade = Pod0Facade()
        let subscriber = RecordingCoreSubscriber()
        let request = ProjectionRequest(scope: .library, offset: 0, maxItems: 20)
        let handle = facade.subscribe(request: request, subscriber: subscriber)

        XCTAssertEqual(subscriber.revisions, [0])

        facade.dispatch(
            command: CommandEnvelope(
                commandId: CommandId(high: 0, low: 1),
                cancellationId: CancellationId(high: 0, low: 2),
                expectedRevision: nil,
                command: .unsupported(wireCode: 77)
            )
        )

        XCTAssertEqual(subscriber.revisions, [0, 1])
        let projection = facade.snapshot(request: request)
        XCTAssertEqual(projection.contractVersion, 8)
        guard case let .library(value) = projection.projection else {
            return XCTFail("Expected a bounded library projection")
        }
        XCTAssertEqual(value.operations.count, 1)
        let unsupportedOperation = value.operations[0]
        XCTAssertEqual(unsupportedOperation.commandId, CommandId(high: 0, low: 1))
        XCTAssertEqual(unsupportedOperation.cancellationId, CancellationId(high: 0, low: 2))
        XCTAssertTrue(unsupportedOperation.stage == OperationStage.failed)
        XCTAssertEqual(unsupportedOperation.failure?.code, .unsupported(wireCode: 77))
        XCTAssertNil(unsupportedOperation.failure?.safeDetail)

        facade.dispatch(
            command: CommandEnvelope(
                commandId: CommandId(high: 0, low: 3),
                cancellationId: CancellationId(high: 0, low: 4),
                expectedRevision: nil,
                command: .subscribeToFeed(feedUrl: "https://example.test/feed")
            )
        )
        facade.dispatch(
            command: CommandEnvelope(
                commandId: CommandId(high: 0, low: 5),
                cancellationId: CancellationId(high: 0, low: 6),
                expectedRevision: nil,
                command: .cancelOperation(cancellationId: CancellationId(high: 0, low: 4))
            )
        )

        XCTAssertTrue(facade.nextHostRequests(maximumCount: 64).isEmpty)
        let cancelledProjection = facade.snapshot(request: request)
        guard case let .library(cancelledValue) = cancelledProjection.projection else {
            return XCTFail("Expected a library projection after cancellation")
        }
        let cancelledCommandID = CommandId(high: 0, low: 3)
        let includesCancelledOperation = cancelledValue.operations.contains { operation in
            let commandMatches = operation.commandId == cancelledCommandID
            let stageMatches = operation.stage == OperationStage.cancelled
            let failureMatches = operation.failure?.code == CoreFailureCode.cancelled
            return commandMatches && stageMatches && failureMatches
        }
        XCTAssertTrue(includesCancelledOperation)

        facade.unsubscribe(subscriptionId: handle)
        facade.dispatch(
            command: CommandEnvelope(
                commandId: CommandId(high: 0, low: 7),
                cancellationId: CancellationId(high: 0, low: 8),
                expectedRevision: nil,
                command: .unsupported(wireCode: 78)
            )
        )
        XCTAssertEqual(subscriber.revisions, [0, 1, 2, 3])
    }

    func testSwiftDecodesRecallProjectionGoldenFixture() throws {
        let fixtureURL = try XCTUnwrap(
            Bundle(for: Self.self).url(
                forResource: "recall-projection-v1",
                withExtension: "properties"
            )
        )
        let fixture = try decodeProperties(at: fixtureURL)
        func number(_ key: String) throws -> UInt64 {
            try XCTUnwrap(UInt64(fixture[key] ?? ""), "Missing numeric fixture value: \(key)")
        }

        let evidence = RecallEvidenceProjection(
            episodeId: EpisodeId(
                high: try number("episode_id_high"),
                low: try number("episode_id_low")
            ),
            podcastId: PodcastId(
                high: try number("podcast_id_high"),
                low: try number("podcast_id_low")
            ),
            generationId: EvidenceGenerationId(
                high: try number("generation_id_high"),
                low: try number("generation_id_low")
            ),
            transcriptVersionId: TranscriptVersionId(
                high: try number("transcript_version_id_high"),
                low: try number("transcript_version_id_low")
            ),
            transcriptContentDigest: ContentDigest(
                word0: try number("content_digest_word_0"),
                word1: try number("content_digest_word_1"),
                word2: try number("content_digest_word_2"),
                word3: try number("content_digest_word_3")
            ),
            spanId: EvidenceSpanId(
                high: try number("span_id_high"),
                low: try number("span_id_low")
            ),
            firstSegmentId: TranscriptSegmentId(
                high: try number("first_segment_id_high"),
                low: try number("first_segment_id_low")
            ),
            lastSegmentId: TranscriptSegmentId(
                high: try number("last_segment_id_high"),
                low: try number("last_segment_id_low")
            ),
            startSegmentOrdinal: UInt32(try number("start_segment_ordinal")),
            endSegmentOrdinalExclusive: UInt32(try number("end_segment_ordinal_exclusive")),
            startMilliseconds: try number("start_milliseconds"),
            endMilliseconds: try number("end_milliseconds"),
            excerpt: try XCTUnwrap(fixture["excerpt"]),
            speakerId: SpeakerId(
                high: try number("speaker_id_high"),
                low: try number("speaker_id_low")
            ),
            provenance: TranscriptProvenance(
                source: .publisher,
                provider: fixture["provenance_provider"],
                sourcePayloadDigest: ContentDigest(
                    word0: try number("source_digest_word_0"),
                    word1: try number("source_digest_word_1"),
                    word2: try number("source_digest_word_2"),
                    word3: try number("source_digest_word_3")
                )
            ),
            score: RecallScoreProjection(
                vectorRrfUnits: try number("vector_rrf_units"),
                lexicalRrfUnits: try number("lexical_rrf_units"),
                totalRrfUnits: try number("total_rrf_units"),
                baseRank: UInt16(try number("base_rank")),
                rerankRank: UInt16(try number("rerank_rank"))
            )
        )
        let projection = RecallResultProjection(
            queryId: RecallQueryId(
                high: try number("query_id_high"),
                low: try number("query_id_low")
            ),
            stage: .ready,
            evidence: [evidence],
            failure: nil,
            operation: nil
        )

        XCTAssertEqual(UInt32(fixture["contract_version"] ?? ""), 8)
        XCTAssertEqual(projection.stage, .ready)
        XCTAssertEqual(projection.evidence.first?.excerpt, fixture["excerpt"])
        XCTAssertEqual(
            projection.evidence.first?.score.totalRrfUnits,
            projection.evidence.first.map { $0.score.vectorRrfUnits + $0.score.lexicalRrfUnits }
        )
    }

    private func decodeProperties(at url: URL) throws -> [String: String] {
        try String(contentsOf: url, encoding: .utf8)
            .split(whereSeparator: \.isNewline)
            .filter { !$0.isEmpty && !$0.hasPrefix("#") }
            .reduce(into: [:]) { result, line in
                let parts = line.split(
                    separator: "=",
                    maxSplits: 1,
                    omittingEmptySubsequences: false
                )
                guard parts.count == 2 else { return }
                result[String(parts[0])] = String(parts[1])
            }
    }
}

private final class RecordingCoreSubscriber: ProjectionSubscriber, @unchecked Sendable {
    private let lock = NSLock()
    private var storedRevisions: [UInt64] = []

    var revisions: [UInt64] {
        lock.withLock { storedRevisions }
    }

    func receive(projection: ProjectionEnvelope) {
        lock.withLock {
            storedRevisions.append(projection.stateRevision.value)
        }
    }
}
