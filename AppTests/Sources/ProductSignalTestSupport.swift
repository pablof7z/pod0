import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

actor RecordingProductSignalSink: ProductSignalSink {
    private(set) var observations: [ProductSignalObservation] = []
    private var waiters: [UUID: Waiter] = [:]

    private struct Waiter {
        let minimumCount: Int
        let continuation: CheckedContinuation<[ProductSignalObservation], Never>
    }

    func record(_ observation: ProductSignalObservation) async {
        observations.append(observation)
        resumeSatisfiedWaiters()
    }

    func deleteAll() async {
        observations.removeAll()
    }

    func captured() -> [ProductSignalObservation] {
        observations
    }

    func waitForCount(
        _ minimumCount: Int,
        timeout: Duration = .seconds(2)
    ) async -> [ProductSignalObservation] {
        if observations.count >= minimumCount { return observations }
        let id = UUID()
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                waiters[id] = Waiter(
                    minimumCount: minimumCount,
                    continuation: continuation
                )
                Task { [weak self] in
                    try? await Task.sleep(for: timeout)
                    await self?.resumeWaiter(id)
                }
            }
        } onCancel: {
            Task { [weak self] in await self?.resumeWaiter(id) }
        }
    }

    private func resumeSatisfiedWaiters() {
        for id in waiters.compactMap({ key, waiter in
            observations.count >= waiter.minimumCount ? key : nil
        }) {
            resumeWaiter(id)
        }
    }

    private func resumeWaiter(_ id: UUID) {
        waiters.removeValue(forKey: id)?.continuation.resume(returning: observations)
    }
}

final class ProductSignalExpectationSink: ProductSignalSink, @unchecked Sendable {
    private let name: ProductSignalName
    private let expectation: XCTestExpectation

    init(name: ProductSignalName, expectation: XCTestExpectation) {
        self.name = name
        self.expectation = expectation
    }

    func record(_ observation: ProductSignalObservation) async {
        if observation.name == name { expectation.fulfill() }
    }

    func deleteAll() async {}
}

@MainActor
final class RecordingPlaybackHost: CorePlaybackHosting {
    private let played: XCTestExpectation
    private(set) var episodeID: EpisodeId?
    private(set) var positionMilliseconds: UInt64 = 0
    private(set) var didPlay = false
    private var state: PlaybackHostState = .idle

    init(played: XCTestExpectation) {
        self.played = played
    }

    func execute(_ request: HostRequest) -> HostObservation {
        switch request {
        case .loadMedia(let episodeID, _, let startPositionMilliseconds):
            self.episodeID = episodeID
            positionMilliseconds = startPositionMilliseconds
            didPlay = false
            state = .prepared
        case .seek(let episodeID, let positionMilliseconds, _, _):
            self.episodeID = episodeID
            self.positionMilliseconds = positionMilliseconds
        case .play(let episodeID, _):
            self.episodeID = episodeID
            didPlay = true
            state = .playing
            played.fulfill()
        case .pause(let episodeID):
            self.episodeID = episodeID
            state = .paused
        case .stopPlayback(let episodeID):
            self.episodeID = episodeID
            state = .idle
        case .setRate, .armNativeTimer, .cancelNativeTimer:
            break
        case .observePlayback:
            break
        default:
            return .failed(
                code: .invalidResponse,
                safeDetail: "Unexpected recall playback request"
            )
        }
        return .playbackObserved(value: PlaybackLifecycleObservation(
            episodeId: episodeID,
            state: state,
            positionMilliseconds: positionMilliseconds,
            durationMilliseconds: 300_000,
            route: .builtIn,
            interruption: .none,
            ended: false
        ))
    }

    func installObservationSink(
        _ sink: @escaping (PlaybackLifecycleObservation) -> Void
    ) {}
}

enum ProductSignalTestSupport {
    static func uniqueFileURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("pod0-product-signal-tests", isDirectory: true)
            .appendingPathComponent("\(UUID().uuidString).json")
    }

    static func dispose(_ url: URL) {
        try? FileManager.default.removeItem(at: url.deletingLastPathComponent())
    }

}
