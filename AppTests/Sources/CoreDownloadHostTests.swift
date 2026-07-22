import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreDownloadHostTests: XCTestCase {
    func testSuccessfulTransferEmitsAcceptedThenStableStagedAndReplaysAfterRelaunch() async throws {
        let root = temporaryDirectory("success")
        defer { try? FileManager.default.removeItem(at: root) }
        let host = makeHost(root: root)
        let request = startEnvelope(
            requestLow: 1,
            attemptLow: 101,
            url: "https://example.test/success"
        )
        var events: [(UInt64, HostObservation)] = []
        let staged = expectation(description: "download staged")

        host.execute(request) { sequence, observation in
            events.append((sequence, observation))
            if case .downloadStaged = observation { staged.fulfill() }
        }
        host.execute(request) { _, _ in XCTFail("Duplicate request executed") }
        await fulfillment(of: [staged], timeout: 2)

        XCTAssertEqual(events.map(\.0), [1, 2])
        guard case let .downloadAccepted(_, _, _, externalTaskKey, resumeKey) = events[0].1 else {
            return XCTFail("Expected accepted observation")
        }
        XCTAssertTrue(externalTaskKey.hasSuffix(":1"))
        XCTAssertNotNil(resumeKey)
        guard case let .downloadStaged(_, _, _, path, byteCount) = events[1].1 else {
            return XCTFail("Expected staged observation")
        }
        XCTAssertEqual(byteCount, UInt64(CoreDownloadURLProtocol.successBytes.count))
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: path)), CoreDownloadURLProtocol.successBytes)

        let relaunched = makeHost(root: root)
        var replay: [(UInt64, HostObservation)] = []
        relaunched.execute(request) { replay.append(($0, $1)) }
        XCTAssertEqual(replay.map(\.0), [2])
        guard case .downloadStaged = replay.first?.1 else {
            return XCTFail("Expected stable staged replay without another transfer")
        }
    }

    func testCancellationUsesExactTaskIdentityAndEmitsRawCancellationOnce() async throws {
        let root = temporaryDirectory("cancel")
        defer { try? FileManager.default.removeItem(at: root) }
        let host = makeHost(root: root)
        let start = startEnvelope(
            requestLow: 2,
            attemptLow: 202,
            url: "https://example.test/hang"
        )
        var acceptedKey: String?
        host.execute(start) { _, observation in
            if case .downloadAccepted(_, _, _, let key, _) = observation { acceptedKey = key }
        }
        try await waitUntil { acceptedKey != nil }
        let cancel = cancelEnvelope(
            requestLow: 3,
            attemptLow: 202,
            externalTaskKey: acceptedKey
        )
        var observations: [HostObservation] = []
        let cancelled = expectation(description: "download cancelled")
        host.execute(cancel) { _, observation in
            observations.append(observation)
            if case .downloadCancelled = observation { cancelled.fulfill() }
        }
        await fulfillment(of: [cancelled], timeout: 2)

        XCTAssertEqual(observations.count, 1)
        guard case let .downloadCancelled(episodeID, intentID, attemptID) = observations[0] else {
            return XCTFail("Expected typed cancellation")
        }
        XCTAssertEqual(episodeID, EpisodeId(high: 7, low: 8))
        XCTAssertEqual(intentID, DownloadIntentId(high: 9, low: 10))
        XCTAssertEqual(attemptID, DownloadAttemptId(high: 11, low: 202))
    }

    func testHTTPFailureReportsRawTypedFailureWithoutStagingBytes() async {
        let root = temporaryDirectory("failure")
        defer { try? FileManager.default.removeItem(at: root) }
        let host = makeHost(root: root)
        let request = startEnvelope(
            requestLow: 4,
            attemptLow: 404,
            url: "https://example.test/failure"
        )
        var terminal: HostObservation?
        let failed = expectation(description: "download failed")
        host.execute(request) { _, observation in
            if case .failed = observation {
                terminal = observation
                failed.fulfill()
            }
        }
        await fulfillment(of: [failed], timeout: 2)

        guard case .failed(code: .providerUnavailable, safeDetail: _) = terminal else {
            return XCTFail("Expected provider-unavailable raw failure")
        }
        XCTAssertNil(host.nativeStore.stagedFile(for: DownloadAttemptId(high: 11, low: 404)))
    }

    func testExistingSessionTaskReattachesByExactIdentityWithoutCreatingDuplicate() async throws {
        let root = temporaryDirectory("reattach")
        defer { try? FileManager.default.removeItem(at: root) }
        let host = makeHost(root: root)
        let request = startEnvelope(
            requestLow: 5,
            attemptLow: 505,
            url: "https://example.test/hang"
        )
        let task = host.session.downloadTask(
            with: URL(string: "https://example.test/hang")!
        )
        task.taskDescription = try XCTUnwrap(CoreDownloadTaskIdentity(request)?.encoded)
        task.resume()
        var accepted: HostObservation?
        host.execute(request) { _, observation in
            if case .downloadAccepted = observation { accepted = observation }
        }
        try await waitUntil { accepted != nil }

        let tasks = await host.session.allTasks
        XCTAssertEqual(tasks.count, 1)
        XCTAssertEqual(tasks.first?.taskIdentifier, task.taskIdentifier)
        host.cancel(
            requestID: request.requestId,
            cancellationID: request.cancellationId
        )
    }

    func testCallbackWithoutActiveDeliveryQueuesExactOrphanFactForRust() async throws {
        let root = temporaryDirectory("orphan")
        defer { try? FileManager.default.removeItem(at: root) }
        let host = makeHost(root: root)
        let request = startEnvelope(
            requestLow: 6,
            attemptLow: 606,
            url: "https://example.test/hang"
        )
        var accepted = false
        host.execute(request) { _, observation in
            if case .downloadAccepted = observation { accepted = true }
        }
        try await waitUntil { accepted && host.tasksByRequest[request.requestId] != nil }
        let task = try XCTUnwrap(host.tasksByRequest[request.requestId])
        let identity = try XCTUnwrap(CoreDownloadTaskIdentity(request))

        host.shutdown()
        host.handleFailure(
            identity: identity,
            taskID: task.taskIdentifier,
            code: .offline,
            safeDetail: "Native transfer failed"
        )
        var orphan: CoreDownloadOrphanObservation?
        host.installOrphanObservationSink { orphan = $0 }

        XCTAssertEqual(orphan?.identity, identity)
        XCTAssertEqual(orphan?.sequenceNumber, 2)
        guard case .failed(code: .offline, safeDetail: _) = orphan?.observation else {
            return XCTFail("Expected typed orphan failure")
        }
        host.cancelledTaskIDs.insert(task.taskIdentifier)
        task.cancel()
    }

    func testBackgroundEventHandoffCompletesOnlyForMatchingSession() {
        let root = temporaryDirectory("handoff")
        defer { try? FileManager.default.removeItem(at: root) }
        let identifier = "io.f7z.podcast.tests.\(UUID().uuidString)"
        let configuration = URLSessionConfiguration.background(withIdentifier: identifier)
        let host = CoreDownloadHost(configuration: configuration, nativeRootURL: root)
        var unknownCompleted = false
        host.handleEventsForBackgroundURLSession(identifier: "other") {
            unknownCompleted = true
        }
        XCTAssertTrue(unknownCompleted)

        var matchingCompleted = false
        host.handleEventsForBackgroundURLSession(identifier: identifier) {
            matchingCompleted = true
        }
        host.handleBackgroundEventsFinished(for: host.session)
        XCTAssertTrue(matchingCompleted)
    }

    private func makeHost(root: URL) -> CoreDownloadHost {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [CoreDownloadURLProtocol.self]
        return CoreDownloadHost(configuration: configuration, nativeRootURL: root)
    }

    private func startEnvelope(
        requestLow: UInt64,
        attemptLow: UInt64,
        url: String
    ) -> HostRequestEnvelope {
        HostRequestEnvelope(
            requestId: HostRequestId(high: 1, low: requestLow),
            commandId: CommandId(high: 2, low: requestLow),
            cancellationId: CancellationId(high: 3, low: requestLow),
            issuedRevision: StateRevision(value: 5),
            deadlineAt: nil,
            request: .startEpisodeDownload(
                episodeId: EpisodeId(high: 7, low: 8),
                intentId: DownloadIntentId(high: 9, low: 10),
                attemptId: DownloadAttemptId(high: 11, low: attemptLow),
                inputVersion: String(repeating: "b", count: 64),
                enclosureUrl: url,
                resumeKey: nil
            )
        )
    }

    private func cancelEnvelope(
        requestLow: UInt64,
        attemptLow: UInt64,
        externalTaskKey: String?
    ) -> HostRequestEnvelope {
        HostRequestEnvelope(
            requestId: HostRequestId(high: 4, low: requestLow),
            commandId: CommandId(high: 5, low: requestLow),
            cancellationId: CancellationId(high: 6, low: requestLow),
            issuedRevision: StateRevision(value: 6),
            deadlineAt: nil,
            request: .cancelEpisodeDownload(
                episodeId: EpisodeId(high: 7, low: 8),
                intentId: DownloadIntentId(high: 9, low: 10),
                attemptId: DownloadAttemptId(high: 11, low: attemptLow),
                externalTaskKey: externalTaskKey
            )
        )
    }

    private func waitUntil(_ condition: @escaping @MainActor () -> Bool) async throws {
        for _ in 0 ..< 100 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Condition did not become true")
    }

    private func temporaryDirectory(_ name: String) -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent(
            "pod0-core-download-\(name)-\(UUID().uuidString)",
            isDirectory: true
        )
    }
}

private final class CoreDownloadURLProtocol: URLProtocol, @unchecked Sendable {
    static let successBytes = Data("url-session-download-bytes".utf8)

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        guard let url = request.url else { return }
        if url.path == "/hang" { return }
        let status = url.path == "/failure" ? 503 : 200
        let response = HTTPURLResponse(
            url: url,
            statusCode: status,
            httpVersion: "HTTP/1.1",
            headerFields: ["Content-Length": String(Self.successBytes.count)]
        )!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Self.successBytes)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}
