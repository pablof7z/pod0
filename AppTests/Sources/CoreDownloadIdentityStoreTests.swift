import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class CoreDownloadIdentityStoreTests: XCTestCase {
    func testTaskIdentityRoundTripsEveryFenceAndRejectsMalformedValues() throws {
        let envelope = startEnvelope(requestLow: 11, attemptLow: 44)
        let identity = try XCTUnwrap(CoreDownloadTaskIdentity(envelope))
        let restored = try XCTUnwrap(CoreDownloadTaskIdentity(encoded: identity.encoded))

        XCTAssertEqual(restored, identity)
        XCTAssertEqual(restored.requestID, envelope.requestId)
        XCTAssertEqual(restored.cancellationID, envelope.cancellationId)
        XCTAssertEqual(restored.observedRequestRevision, envelope.issuedRevision)
        XCTAssertEqual(restored.episodeID, EpisodeId(high: 7, low: 8))
        XCTAssertEqual(restored.intentID, DownloadIntentId(high: 9, low: 10))
        XCTAssertEqual(restored.attemptID, DownloadAttemptId(high: 43, low: 44))
        XCTAssertNil(CoreDownloadTaskIdentity(encoded: "pod0-download-v1:not-base64"))
        XCTAssertNil(CoreDownloadTaskIdentity(encoded: "legacy-task"))
    }

    func testNativeStoreStagesResumeDataAndRemovesOnlyValidatedCoreArtifact() throws {
        let root = temporaryDirectory("native-store")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let nativeRoot = root.appendingPathComponent("native", isDirectory: true)
        let store = CoreDownloadNativeStore(rootURL: nativeRoot)
        let attemptID = DownloadAttemptId(high: 3, low: 4)
        let source = root.appendingPathComponent("source.tmp")
        let bytes = Data("native-staged-audio".utf8)
        try bytes.write(to: source)

        let (staged, byteCount) = try store.stage(source, for: attemptID)
        XCTAssertEqual(byteCount, UInt64(bytes.count))
        XCTAssertEqual(try Data(contentsOf: staged), bytes)
        XCTAssertEqual(store.stagedFile(for: attemptID), staged)

        let resume = Data("opaque-resume-data".utf8)
        store.saveResumeData(resume, for: attemptID)
        let resumeKey = store.resumeKey(for: attemptID)
        XCTAssertEqual(store.resumeData(for: resumeKey), resume)
        XCTAssertNil(store.resumeData(for: "../foreign.resume"))

        let core = root.appendingPathComponent("pod0.core.sqlite")
        let downloads = root.appendingPathComponent(
            "pod0.core.sqlite.downloads/v1",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: downloads, withIntermediateDirectories: true)
        let artifact = downloads.appendingPathComponent("safe.media")
        try bytes.write(to: artifact)
        try store.removeArtifact(coreStoreURL: core, artifactKey: "v1/safe.media")
        XCTAssertFalse(FileManager.default.fileExists(atPath: artifact.path))
        XCTAssertNoThrow(try store.removeArtifact(
            coreStoreURL: core,
            artifactKey: "v1/safe.media"
        ))
        XCTAssertThrowsError(try store.removeArtifact(
            coreStoreURL: core,
            artifactKey: "../outside.media"
        ))

        store.removeNativeFiles(for: attemptID)
        XCTAssertNil(store.stagedFile(for: attemptID))
        XCTAssertNil(store.resumeData(for: resumeKey))
    }

    private func startEnvelope(requestLow: UInt64, attemptLow: UInt64) -> HostRequestEnvelope {
        HostRequestEnvelope(
            requestId: HostRequestId(high: 1, low: requestLow),
            commandId: CommandId(high: 2, low: requestLow),
            cancellationId: CancellationId(high: 3, low: requestLow),
            issuedRevision: StateRevision(value: 5),
            deadlineAt: nil,
            request: .startEpisodeDownload(
                episodeId: EpisodeId(high: 7, low: 8),
                intentId: DownloadIntentId(high: 9, low: 10),
                attemptId: DownloadAttemptId(high: 43, low: attemptLow),
                inputVersion: String(repeating: "a", count: 64),
                enclosureUrl: "https://example.test/audio.mp3",
                resumeKey: nil
            )
        )
    }

    private func temporaryDirectory(_ name: String) -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent(
            "pod0-\(name)-\(UUID().uuidString)",
            isDirectory: true
        )
    }
}
