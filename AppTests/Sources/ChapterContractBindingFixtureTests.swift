import Pod0Core
import XCTest

final class ChapterContractBindingFixtureTests: XCTestCase {
    func testSwiftQualifiesCanonicalChapterGoldenFixture() throws {
        let fixture = try loadFixture()
        let request = try makeRequest(fixture)
        let qualified = projectChapterContract(
            request: request,
            scope: .chapters,
            offset: 0,
            maxItems: 1
        )
        guard case let .qualified(receipt: receipt, artifact: artifact) = qualified else {
            return XCTFail("Valid chapter fixture was rejected")
        }

        XCTAssertEqual(fixture["fixture_version"], "1")
        XCTAssertEqual(UInt32(fixture["contract_version"] ?? ""), 24)
        XCTAssertEqual(fixture["unknown_future_field"], "ignored-by-v1-readers")
        XCTAssertEqual(receipt.artifactId, try id("expected_artifact_id", fixture, ChapterArtifactId.init))
        XCTAssertEqual(receipt.contentDigest, try digest("expected_content_digest", fixture))
        XCTAssertEqual(receipt.integrityDigest, try digest("expected_integrity_digest", fixture))
        XCTAssertEqual(receipt.commandFingerprint, try digest("expected_command_fingerprint", fixture))
        XCTAssertEqual(
            receipt.selectionRevision.value,
            try number("expected_committed_selection_revision", fixture)
        )
        XCTAssertEqual(receipt.chapterCount, UInt32(fixture["chapter_count"] ?? ""))
        XCTAssertEqual(receipt.adSpanCount, UInt32(fixture["ad_span_count"] ?? ""))
        XCTAssertEqual(artifact.chapters.count, 1)
        XCTAssertEqual(artifact.chapters[0].title, fixture["chapter_0_expected_title"])
        XCTAssertEqual(
            artifact.chapters[0].chapterId,
            try id("expected_chapter_0_id", fixture, ChapterId.init)
        )
        XCTAssertEqual(
            artifact.chapters[0].effectiveEndMilliseconds,
            try number("chapter_0_expected_effective_end_milliseconds", fixture)
        )
        XCTAssertTrue(artifact.hasMore)
        guard case .publisherEnriched = try XCTUnwrap(artifact.summary).provenance.source else {
            return XCTFail("Chapter provenance source changed")
        }

        let ads = projectChapterContract(
            request: request,
            scope: .adSpans,
            offset: 0,
            maxItems: 20
        )
        guard case let .qualified(receipt: _, artifact: adArtifact) = ads else {
            return XCTFail("Valid ad-span fixture was rejected")
        }
        XCTAssertEqual(adArtifact.adSpans.count, 1)
        XCTAssertEqual(
            adArtifact.adSpans[0].adSpanId,
            try id("expected_ad_span_0_id", fixture, AdSpanId.init)
        )
        guard case .midroll = adArtifact.adSpans[0].kind else {
            return XCTFail("Ad kind changed")
        }
    }

    func testMigrationFailuresAreStateShapedAcrossSwiftBinding() {
        let missing = "/definitely-missing-pod0-chapter-source"
        let inspected = inspectLegacyChapterMigration(
            sourceDatabasePath: missing,
            artifactRootPath: missing
        )
        XCTAssertEqual(inspected.stage, .blocked)
        XCTAssertEqual(inspected.failure?.code, .storageUnavailable)
        XCTAssertNil(inspected.report)
        XCTAssertNil(inspected.rollbackExport)

        let status = readActiveLegacyChapterMigration(targetPath: missing)
        XCTAssertEqual(status.stage, .blocked)
        XCTAssertEqual(status.failure?.diagnosticCode, "storage_sqlite")

        let rollback = exportLegacyChapterRollback(
            targetPath: missing,
            legacyBackupRootPath: missing,
            exportRootPath: missing
        )
        XCTAssertEqual(rollback.stage, .blocked)
        XCTAssertNil(rollback.rollbackExport)
    }

    private func makeRequest(_ fixture: [String: String]) throws -> ChapterContractRequest {
        let chapterCount = Int(try number("chapter_count", fixture))
        let chapters = try (0..<chapterCount).map { index in
            let prefix = "chapter_\(index)"
            let sourceEpisodeID: EpisodeId? = if fixture["\(prefix)_source_episode"] == "none" {
                nil
            } else {
                try id("\(prefix)_source_episode_id", fixture, EpisodeId.init)
            }
            return ChapterInput(
                startMilliseconds: try number("\(prefix)_start_milliseconds", fixture),
                endMilliseconds: try optionalNumber("\(prefix)_end_milliseconds", fixture),
                title: try value("\(prefix)_title", fixture),
                summary: try optionalValue("\(prefix)_summary", fixture),
                imageUrl: try optionalValue("\(prefix)_image_url", fixture),
                linkUrl: try optionalValue("\(prefix)_link_url", fixture),
                includeInTableOfContents: try value("\(prefix)_include_in_toc", fixture) == "true",
                sourceEpisodeId: sourceEpisodeID
            )
        }
        let adSpanCount = Int(try number("ad_span_count", fixture))
        let adSpans = try (0..<adSpanCount).map { index in
            let prefix = "ad_span_\(index)"
            return AdSpanInput(
                startMilliseconds: try number("\(prefix)_start_milliseconds", fixture),
                endMilliseconds: try number("\(prefix)_end_milliseconds", fixture),
                kind: .midroll
            )
        }
        return ChapterContractRequest(
            commandId: try id("command_id", fixture, CommandId.init),
            expectedSelectionRevision: StateRevision(
                value: try number("expected_selection_revision", fixture)
            ),
            artifact: ChapterArtifactInput(
                episodeId: try id("episode_id", fixture, EpisodeId.init),
                podcastId: try id("podcast_id", fixture, PodcastId.init),
                sourceRevision: try value("source_revision", fixture),
                provenance: ChapterArtifactProvenance(
                    source: .publisherEnriched,
                    provider: fixture["provider"],
                    model: fixture["model"],
                    policyVersion: UInt32(try number("policy_version", fixture)),
                    sourcePayloadDigest: try digest("source_payload_digest", fixture),
                    transcriptVersionId: try id(
                        "transcript_version_id",
                        fixture,
                        TranscriptVersionId.init
                    ),
                    transcriptContentDigest: try digest("transcript_content_digest", fixture),
                    legacyImport: nil
                ),
                generatedAt: UnixTimestampMilliseconds(
                    value: Int64(try number("generated_at_milliseconds", fixture))
                ),
                durationMilliseconds: try number("duration_milliseconds", fixture),
                chapters: chapters,
                adSpanEvaluation: .evaluated,
                adSpans: adSpans
            )
        )
    }

    private func loadFixture() throws -> [String: String] {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(
            forResource: "chapter-contract-v1",
            withExtension: "properties"
        ))
        return try String(contentsOf: url, encoding: .utf8)
            .split(whereSeparator: \.isNewline)
            .filter { !$0.isEmpty && !$0.hasPrefix("#") }
            .reduce(into: [:]) { values, line in
                let parts = line.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
                guard parts.count == 2 else { return }
                values[String(parts[0])] = String(parts[1])
            }
    }

    private func value(_ key: String, _ fixture: [String: String]) throws -> String {
        try XCTUnwrap(fixture[key], "Missing fixture value: \(key)")
    }

    private func optionalValue(_ key: String, _ fixture: [String: String]) throws -> String? {
        let value = try value(key, fixture)
        return value == "none" ? nil : value
    }

    private func optionalNumber(_ key: String, _ fixture: [String: String]) throws -> UInt64? {
        try optionalValue(key, fixture).flatMap(UInt64.init)
    }

    private func number(_ key: String, _ fixture: [String: String]) throws -> UInt64 {
        try XCTUnwrap(UInt64(fixture[key] ?? ""), "Missing numeric fixture value: \(key)")
    }

    private func id<T>(
        _ prefix: String,
        _ fixture: [String: String],
        _ build: (UInt64, UInt64) -> T
    ) throws -> T {
        build(try number("\(prefix)_high", fixture), try number("\(prefix)_low", fixture))
    }

    private func digest(_ prefix: String, _ fixture: [String: String]) throws -> ContentDigest {
        ContentDigest(
            word0: try number("\(prefix)_word_0", fixture),
            word1: try number("\(prefix)_word_1", fixture),
            word2: try number("\(prefix)_word_2", fixture),
            word3: try number("\(prefix)_word_3", fixture)
        )
    }
}
