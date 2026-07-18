import XCTest
@testable import Podcastr

@MainActor
final class ArtifactVerificationExecutorTests: XCTestCase {
    func testLargeVerificationYieldsMainActorAndRunsFileAccessOffMain() async throws {
        let url = temporaryURL()
        let data = Data(repeating: 0x5a, count: 4 * 1_048_576)
        try data.write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let access = FileAccessRecorder()
        let gate = DispatchSemaphore(value: 0)
        let verifier = ArtifactVerificationExecutor(onFileAccess: {
            access.record(isMainThread: Thread.isMainThread)
            _ = gate.wait(timeout: .now() + 2)
        })
        let request = makeRequest(
            url: url,
            hash: ArtifactRepository.hash(data),
            size: Int64(data.count)
        )

        let task = Task { await verifier.verify(request) }
        for _ in 0..<100 where access.count == 0 { await Task.yield() }
        XCTAssertEqual(access.count, 1)
        XCTAssertTrue(Thread.isMainThread)
        gate.signal()
        let result = await task.value

        XCTAssertEqual(result.integrity, .available)
        XCTAssertEqual(result.observedSize, Int64(data.count))
        XCTAssertEqual(access.mainThreadObservations, [false])
    }

    func testExactIdentityCacheAvoidsDuplicateFileRead() async throws {
        let url = temporaryURL()
        let data = Data("verified artifact".utf8)
        try data.write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let access = FileAccessRecorder()
        let verifier = ArtifactVerificationExecutor(onFileAccess: {
            access.record(isMainThread: Thread.isMainThread)
        })
        let request = makeRequest(
            url: url,
            hash: ArtifactRepository.hash(data),
            size: Int64(data.count)
        )

        let first = await verifier.verify(request)
        let second = await verifier.verify(request)

        XCTAssertEqual(first, second)
        XCTAssertEqual(access.count, 1)
    }

    func testCacheInvalidatesWhenFileIdentityChanges() async throws {
        let url = temporaryURL()
        let original = Data("first version".utf8)
        let replacement = Data("other version".utf8)
        XCTAssertEqual(original.count, replacement.count)
        try original.write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let access = FileAccessRecorder()
        let verifier = ArtifactVerificationExecutor(onFileAccess: {
            access.record(isMainThread: Thread.isMainThread)
        })
        let request = makeRequest(
            url: url,
            hash: ArtifactRepository.hash(original),
            size: Int64(original.count)
        )
        let initial = await verifier.verify(request)
        XCTAssertEqual(initial.integrity, .available)
        try replacement.write(to: url)
        try FileManager.default.setAttributes(
            [.modificationDate: Date().addingTimeInterval(10)],
            ofItemAtPath: url.path
        )

        let replaced = await verifier.verify(request)

        XCTAssertEqual(replaced.integrity, .hashMismatch)
        XCTAssertEqual(access.count, 2)
    }

    func testMissingSizeMismatchAndHashMismatchAreTyped() async throws {
        let missingURL = temporaryURL()
        let verifier = ArtifactVerificationExecutor()
        let missing = await verifier.verify(makeRequest(url: missingURL, hash: "hash", size: 1))
        XCTAssertEqual(missing.integrity, .missing)

        let url = temporaryURL()
        let data = Data("artifact".utf8)
        try data.write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let sizeMismatch = await verifier.verify(makeRequest(
            url: url, hash: ArtifactRepository.hash(data), size: 999
        ))
        let hashMismatch = await verifier.verify(makeRequest(
            url: url, hash: "not-the-hash", size: Int64(data.count)
        ))

        XCTAssertEqual(sizeMismatch.integrity, .sizeMismatch)
        XCTAssertEqual(hashMismatch.integrity, .hashMismatch)
    }

    func testFileReplacementDuringReadCannotCommitAvailableEvidence() async throws {
        let url = temporaryURL()
        let original = Data("immutable evidence".utf8)
        let replacement = Data("replaced evidence!".utf8)
        XCTAssertEqual(original.count, replacement.count)
        try original.write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let verifier = ArtifactVerificationExecutor(onFingerprintComplete: { verifiedURL in
            try? replacement.write(to: verifiedURL)
            try? FileManager.default.setAttributes(
                [.modificationDate: Date().addingTimeInterval(10)],
                ofItemAtPath: verifiedURL.path
            )
        })

        let result = await verifier.verify(makeRequest(
            url: url,
            hash: ArtifactRepository.hash(original),
            size: Int64(original.count)
        ))

        XCTAssertEqual(result.integrity, .changedDuringRead)
        XCTAssertNil(result.verifiedAt)
    }

    func testCancellationDiscardsVerificationResult() async throws {
        let url = temporaryURL()
        let data = Data(repeating: 0x7f, count: 2 * 1_048_576)
        try data.write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let gate = DispatchSemaphore(value: 0)
        let access = FileAccessRecorder()
        let verifier = ArtifactVerificationExecutor(onFileAccess: {
            access.record(isMainThread: Thread.isMainThread)
            _ = gate.wait(timeout: .now() + 2)
        })
        let task = Task {
            await verifier.verify(makeRequest(
                url: url,
                hash: ArtifactRepository.hash(data),
                size: Int64(data.count)
            ))
        }
        for _ in 0..<100 where access.count == 0 { await Task.yield() }
        task.cancel()
        gate.signal()

        let result = await task.value
        XCTAssertEqual(result.integrity, .cancelled)
        XCTAssertNil(result.verifiedAt)
    }

    private func makeRequest(
        url: URL,
        hash: String?,
        size: Int64?
    ) -> ArtifactVerificationRequest {
        ArtifactVerificationRequest(
            artifactID: "test-artifact",
            location: url,
            expectedHash: hash,
            expectedSize: size,
            schemaVersion: 1,
            cancellationID: UUID()
        )
    }

    private func temporaryURL() -> URL {
        URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("artifact-verification-\(UUID().uuidString)")
    }
}

private final class FileAccessRecorder: @unchecked Sendable {
    private let lock = NSLock()
    private var observations: [Bool] = []

    var count: Int { lock.withLock { observations.count } }
    var mainThreadObservations: [Bool] { lock.withLock { observations } }

    func record(isMainThread: Bool) {
        lock.withLock { observations.append(isMainThread) }
    }
}
