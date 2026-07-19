import Foundation
import XCTest
@testable import Podcastr

actor RecordingProductSignalSink: ProductSignalSink {
    private(set) var observations: [ProductSignalObservation] = []

    func record(_ observation: ProductSignalObservation) async {
        observations.append(observation)
    }

    func deleteAll() async {
        observations.removeAll()
    }

    func captured() -> [ProductSignalObservation] {
        observations
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

    static func eventually(
        timeoutNanoseconds: UInt64 = 1_000_000_000,
        _ condition: @escaping @Sendable () async -> Bool
    ) async -> Bool {
        let clock = ContinuousClock()
        let deadline = clock.now + .nanoseconds(Int64(timeoutNanoseconds))
        while clock.now < deadline {
            if await condition() { return true }
            try? await Task.sleep(for: .milliseconds(10))
        }
        return await condition()
    }
}
