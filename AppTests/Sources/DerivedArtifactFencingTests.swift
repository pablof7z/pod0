import Foundation
import XCTest
@testable import Podcastr

final class DerivedArtifactFencingTests: XCTestCase {
    private var rootURL: URL!
    private var databaseURL: URL!

    override func setUp() {
        super.setUp()
        rootURL = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        databaseURL = Persistence.episodeStoreURL(for: AppStateTestSupport.uniqueTempFileURL())
    }

    override func tearDown() {
        if let rootURL { try? FileManager.default.removeItem(at: rootURL) }
        if let databaseURL {
            for suffix in ["", "-wal", "-shm"] {
                try? FileManager.default.removeItem(
                    at: URL(fileURLWithPath: databaseURL.path + suffix)
                )
            }
        }
        rootURL = nil
        databaseURL = nil
        super.tearDown()
    }

    func testStaleChapterAttemptCannotSelectArtifacts() throws {
        let subject = UUID()
        let now = Date(timeIntervalSince1970: 1_000)
        let jobs = JobStore(fileURL: databaseURL)
        let repository = ArtifactRepository(fileURL: databaseURL)
        let desired = DesiredJob(
            idempotencyKey: "chapters:\(subject):v1",
            kind: .chapterArtifacts,
            subjectID: subject,
            inputVersion: "v1",
            resourceClass: .utilityLLM
        )
        _ = try jobs.ensureJob(desired, notBefore: now)
        let first = try XCTUnwrap(try jobs.claimDueJobs(
            resourceClass: .utilityLLM,
            capacity: 1,
            now: now,
            owner: "first",
            leaseDuration: 1
        ).first)
        let firstToken = try XCTUnwrap(first.leaseToken)
        try jobs.markRunning(id: first.id, leaseToken: firstToken, now: now)

        let staging = DerivedArtifactStagingStore(rootURL: rootURL)
        let output = ChapterCompilationOutput(
            chapters: [],
            ads: [],
            chapterOrigin: .generated
        )
        let manifestHash = try staging.stageChapters(
            output,
            episodeID: subject,
            inputVersion: "v1",
            leaseToken: firstToken
        )
        let verified = try XCTUnwrap(staging.verifiedChapters(
            episodeID: subject,
            inputVersion: "v1",
            leaseToken: firstToken,
            manifestHash: manifestHash
        ))
        let locations = try staging.promote(verified, episodeID: subject)

        try jobs.reclaimExpiredLeases(now: now.addingTimeInterval(2))
        let staleRecords = records(
            subject: subject,
            input: "v1",
            verified: verified,
            locations: locations
        )
        XCTAssertThrowsError(try repository.commit(
            staleRecords,
            completingJobID: first.id,
            leaseToken: firstToken
        ))
        XCTAssertNil(try repository.current(kind: .chapters, subjectID: subject))
        XCTAssertNil(try repository.current(kind: .adSegments, subjectID: subject))
    }

    func testChapterAndEmptyAdArtifactsVersionIndependently() throws {
        let subject = UUID()
        let jobs = JobStore(fileURL: databaseURL)
        let repository = ArtifactRepository(fileURL: databaseURL)
        _ = try jobs.ensureJob(DesiredJob(
            idempotencyKey: "chapters:\(subject):v2",
            kind: .chapterArtifacts,
            subjectID: subject,
            inputVersion: "v2",
            resourceClass: .utilityLLM
        ))
        let job = try XCTUnwrap(try jobs.claimDueJobs(
            resourceClass: .utilityLLM,
            capacity: 1,
            now: Date(),
            owner: "current",
            leaseDuration: 60
        ).first)
        let token = try XCTUnwrap(job.leaseToken)
        let staging = DerivedArtifactStagingStore(rootURL: rootURL)
        let output = ChapterCompilationOutput(
            chapters: [.init(startTime: 0, title: "Opening", isAIGenerated: true)],
            ads: [],
            chapterOrigin: .generated
        )
        let manifestHash = try staging.stageChapters(
            output,
            episodeID: subject,
            inputVersion: "v2",
            leaseToken: token
        )
        let verified = try XCTUnwrap(staging.verifiedChapters(
            episodeID: subject,
            inputVersion: "v2",
            leaseToken: token,
            manifestHash: manifestHash
        ))
        let locations = try staging.promote(verified, episodeID: subject)
        try repository.commit(
            records(subject: subject, input: "v2", verified: verified, locations: locations),
            completingJobID: job.id,
            leaseToken: token
        )

        let chapters = try XCTUnwrap(repository.current(kind: .chapters, subjectID: subject))
        let ads = try XCTUnwrap(repository.current(kind: .adSegments, subjectID: subject))
        XCTAssertNotEqual(chapters.outputVersion, ads.outputVersion)
        XCTAssertEqual(staging.loadChapters(at: locations.chapters)?.count, 1)
        XCTAssertEqual(staging.loadAds(at: locations.ads), [])
    }

    func testArtifactHistoryRetainsPriorVersionAndSelectsLatest() throws {
        let subject = UUID()
        let repository = ArtifactRepository(fileURL: databaseURL)
        let first = ArtifactRecord(
            kind: .transcript,
            subjectID: subject,
            inputVersion: "audio-v1",
            outputVersion: "transcript-v1",
            contentHash: "hash-v1",
            location: "/tmp/transcript-v1.json",
            origin: "test",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date(timeIntervalSince1970: 1_000)
        )
        let second = ArtifactRecord(
            kind: .transcript,
            subjectID: subject,
            inputVersion: "audio-v2",
            outputVersion: "transcript-v2",
            contentHash: "hash-v2",
            location: "/tmp/transcript-v2.json",
            origin: "test",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date(timeIntervalSince1970: 2_000)
        )

        try repository.adopt(first)
        try repository.adopt(second)

        XCTAssertEqual(
            try repository.current(kind: .transcript, subjectID: subject)?.outputVersion,
            "transcript-v2"
        )
        let history = try repository.history(kind: .transcript, subjectID: subject)
        XCTAssertEqual(history.map(\.outputVersion), ["transcript-v2", "transcript-v1"])
        XCTAssertEqual(history.map(\.integrity), [.available, .stale])
    }

    private func records(
        subject: UUID,
        input: String,
        verified: VerifiedChapterArtifacts,
        locations: (chapters: URL, ads: URL)
    ) -> [ArtifactRecord] {
        let date = Date()
        return [
            ArtifactRecord(
                kind: .chapters,
                subjectID: subject,
                inputVersion: input,
                outputVersion: verified.chaptersHash,
                contentHash: verified.chaptersHash,
                location: locations.chapters.path,
                origin: "generated",
                schemaVersion: 1,
                integrity: .available,
                verifiedAt: date
            ),
            ArtifactRecord(
                kind: .adSegments,
                subjectID: subject,
                inputVersion: input,
                outputVersion: verified.adsHash,
                contentHash: verified.adsHash,
                location: locations.ads.path,
                origin: "generated",
                schemaVersion: 1,
                integrity: .available,
                verifiedAt: date
            ),
        ]
    }
}
