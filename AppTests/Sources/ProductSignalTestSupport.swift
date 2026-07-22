import Foundation
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
