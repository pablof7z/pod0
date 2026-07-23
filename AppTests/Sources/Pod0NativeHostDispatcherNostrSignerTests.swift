import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class Pod0NativeHostDispatcherNostrSignerTests: XCTestCase {
    func testSignerCancellationEmitsOnceAndRejectsLateCredentialResult() async {
        let signer = ControlledNostrSignerHost()
        let dispatcher = makeDispatcher(signer: signer)
        let request = envelope(id: 1)
        var observations: [HostObservationEnvelope] = []

        dispatcher.execute(request) { observations.append($0) }
        await signer.waitUntilStarted()
        dispatcher.cancel(cancellationID: request.cancellationId)
        await signer.finish(.nostrSignerCredentialReady(
            accountId: SignerAccountId(high: 7, low: 8),
            publicKeyHex: String(repeating: "a", count: 64)
        ))
        await Task.yield()

        XCTAssertEqual(observations.map(\.observation), [.cancelled])
    }

    func testSignerDispatcherTeardownSuppressesLateCredentialResult() async {
        let signer = ControlledNostrSignerHost()
        let dispatcher = makeDispatcher(signer: signer)
        var observations: [HostObservationEnvelope] = []

        dispatcher.execute(envelope(id: 2)) { observations.append($0) }
        await signer.waitUntilStarted()
        dispatcher.shutdown()
        await signer.finish(.failed(code: .platformFailure, safeDetail: nil))
        await Task.yield()

        XCTAssertTrue(observations.isEmpty)
    }

    private func makeDispatcher(
        signer: any CoreNostrSignerHosting
    ) -> Pod0NativeHostDispatcher {
        Pod0NativeHostDispatcher(
            feedHost: NoopSignerFeedHost(),
            nostrSignerHost: signer,
            playbackHost: NoopSignerPlaybackHost()
        )
    }

    private func envelope(id: UInt64) -> HostRequestEnvelope {
        HostRequestEnvelope(
            requestId: HostRequestId(high: 1, low: id),
            commandId: CommandId(high: 1, low: id),
            cancellationId: CancellationId(high: 1, low: id),
            issuedRevision: StateRevision(value: 1),
            deadlineAt: nil,
            request: .restoreNostrSignerCredential(
                accountId: SignerAccountId(high: 7, low: 8),
                expectedAuthorHex: String(repeating: "a", count: 64)
            )
        )
    }
}

private actor ControlledNostrSignerHost: CoreNostrSignerHosting {
    private var continuation: CheckedContinuation<HostObservation, Never>?
    private var started = false

    func execute(_ request: HostRequest) async -> HostObservation {
        started = true
        return await withCheckedContinuation { continuation = $0 }
    }

    func waitUntilStarted() async {
        while !started { await Task.yield() }
    }

    func finish(_ observation: HostObservation) {
        continuation?.resume(returning: observation)
        continuation = nil
    }
}

private struct NoopSignerFeedHost: CoreFeedHosting {
    func fetch(
        feedURL: String,
        entityTag: String?,
        lastModified: String?,
        maximumResponseBytes: UInt64,
        deadline: Date?
    ) async -> HostObservation {
        .failed(code: .platformFailure, safeDetail: nil)
    }
}

@MainActor
private final class NoopSignerPlaybackHost: CorePlaybackHosting {
    func execute(_ request: HostRequest) -> HostObservation {
        .failed(code: .platformFailure, safeDetail: nil)
    }

    func installObservationSink(_ sink: @escaping (PlaybackLifecycleObservation) -> Void) {}
}
