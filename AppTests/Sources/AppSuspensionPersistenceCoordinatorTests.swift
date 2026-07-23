import XCTest
@testable import Podcastr

@MainActor
final class AppSuspensionPersistenceCoordinatorTests: XCTestCase {
    func testConcurrentSuspensionRequestsShareOneBackgroundLeaseAndFlush() async {
        let background = RecordingBackgroundExecutionHost()
        let coordinator = AppSuspensionPersistenceCoordinator(
            backgroundExecution: background
        )
        var flushCount = 0

        coordinator.persistForSuspension {
            flushCount += 1
            return true
        }
        coordinator.persistForSuspension {
            flushCount += 1
            return true
        }
        await coordinator.waitUntilIdle()

        XCTAssertEqual(flushCount, 1)
        XCTAssertEqual(background.beginCount, 1)
        XCTAssertEqual(background.endedTokens, [1])
    }

    func testExpirationEndsLeaseExactlyOnceWhileFlushFinishesSafely() async {
        let background = RecordingBackgroundExecutionHost()
        let gate = SuspensionFlushGate()
        let coordinator = AppSuspensionPersistenceCoordinator(
            backgroundExecution: background
        )
        coordinator.persistForSuspension {
            await gate.wait()
            return !Task.isCancelled
        }
        await gate.waitUntilEntered()

        background.expire()
        XCTAssertEqual(background.endedTokens, [1])
        await gate.release()
        await coordinator.waitUntilIdle()

        XCTAssertEqual(background.endedTokens, [1])
    }
}

@MainActor
private final class RecordingBackgroundExecutionHost: AppBackgroundExecutionHosting {
    private(set) var beginCount = 0
    private(set) var endedTokens: [Int] = []
    private var expirationHandler: (@MainActor @Sendable () -> Void)?

    func begin(
        expirationHandler: @escaping @MainActor @Sendable () -> Void
    ) -> Int? {
        beginCount += 1
        self.expirationHandler = expirationHandler
        return beginCount
    }

    func end(_ token: Int) {
        endedTokens.append(token)
    }

    func expire() {
        expirationHandler?()
    }
}

private actor SuspensionFlushGate {
    private var entered = false
    private var entryWaiters: [CheckedContinuation<Void, Never>] = []
    private var releaseWaiter: CheckedContinuation<Void, Never>?

    func wait() async {
        entered = true
        for waiter in entryWaiters { waiter.resume() }
        entryWaiters.removeAll()
        await withCheckedContinuation { releaseWaiter = $0 }
    }

    func waitUntilEntered() async {
        guard !entered else { return }
        await withCheckedContinuation { entryWaiters.append($0) }
    }

    func release() {
        releaseWaiter?.resume()
        releaseWaiter = nil
    }
}
