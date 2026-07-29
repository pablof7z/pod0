import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

/// Queued feed host used by shared-core vertical slice tests. Each fetch
/// consumes the next queued observation; an exhausted queue reports a
/// bounded platform failure so Rust parks the workflow in a retry stage
/// instead of hanging the pump.
actor QueuedCoreFeedHost: CoreFeedHosting {
    struct Request: Sendable {
        let feedURL: String
        let entityTag: String?
        let lastModified: String?
        let maximumResponseBytes: UInt64
    }

    private var responses: [HostObservation]
    private var requests: [Request] = []

    init(_ responses: [HostObservation]) {
        self.responses = responses
    }

    func fetch(
        feedURL: String,
        entityTag: String?,
        lastModified: String?,
        maximumResponseBytes: UInt64,
        deadline: Date?
    ) async -> HostObservation {
        requests.append(Request(
            feedURL: feedURL,
            entityTag: entityTag,
            lastModified: lastModified,
            maximumResponseBytes: maximumResponseBytes
        ))
        guard !responses.isEmpty else {
            return .failed(code: .platformFailure, safeDetail: "No queued test response")
        }
        return responses.removeFirst()
    }

    func recordedRequests() -> [Request] { requests }
}

/// Feed host whose single response is withheld until the test releases it.
/// Proves the commit-before-fetch contract: while the gate is closed no feed
/// observation can exist, so anything visible after `addSubscription`
/// returns was committed durably before the fetch.
actor GatedCoreFeedHost: CoreFeedHosting {
    private let observation: HostObservation
    private var released = false
    private var gate: CheckedContinuation<Void, Never>?

    init(_ observation: HostObservation) {
        self.observation = observation
    }

    func fetch(
        feedURL _: String,
        entityTag _: String?,
        lastModified _: String?,
        maximumResponseBytes _: UInt64,
        deadline _: Date?
    ) async -> HostObservation {
        if !released {
            await withCheckedContinuation { gate = $0 }
        }
        return observation
    }

    func release() {
        released = true
        gate?.resume()
        gate = nil
    }
}

extension AppStateTestSupport {
    /// Drives the shared-core host pump until Rust owes no in-flight feed
    /// fetch and the resulting library projection is applied to the native
    /// mirror. From contract version 53 feed commands succeed at durable
    /// commit, so tests asserting hydrated state must drive this pump first.
    ///
    /// Deterministic: every pass awaits the real pending work (the command
    /// tail, the dispatcher drain, in-flight host tasks, and the projection
    /// application task) and re-reads the facade's own durable workflow rows
    /// as ground truth. No sleeps and no timed polling; the yield only lets
    /// already-enqueued main-actor work run before re-inspecting.
    @MainActor
    static func settleSharedFeedWork(
        _ store: AppStateStore,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async {
        guard let client = store.sharedLibrary else {
            return XCTFail("Shared library unavailable", file: file, line: line)
        }
        let dispatcher = client.dispatcher
        func activeFeedFetchTask() -> Task<Void, Never>? {
            // Only feed fetches are awaited: other active tasks (for example
            // core-wake sleepers for scheduled retries) park for minutes by
            // design and are not part of the feed pump.
            dispatcher.activeTasks.values.first { active in
                if case .fetchFeed = active.envelope.request { return true }
                return false
            }?.task
        }
        for _ in 0 ..< 10_000 {
            await client.subscriptionTask?.value
            await client.initialProjectionTask?.value
            await client.coreCommandTail?.value
            await dispatcher.requestDrainTask?.value
            if let feedTask = activeFeedFetchTask() {
                await feedTask.value
                continue
            }
            let envelope = await client.coreSnapshot(ProjectionRequest(
                scope: .library,
                offset: 0,
                maxItems: 1
            ))
            guard case .library(let page) = envelope.projection else {
                return XCTFail("Expected a library projection", file: file, line: line)
            }
            let fetchPending = page.feedFetches.contains { $0.stage == .requested }
            let dispatcherIdle = activeFeedFetchTask() == nil
                && dispatcher.requestDrainTask == nil
                && client.coreCommandTail == nil
            let projectionApplied = client.lastLibraryRevision
                >= envelope.stateRevision.value
            if !fetchPending, dispatcherIdle, projectionApplied {
                await client.libraryProjectionTask?.value
                return
            }
            if fetchPending, dispatcherIdle {
                // A workflow row is due but no drain is running (for example
                // a retry that just became claimable): kick the pump.
                client.executePendingHostRequests()
            }
            await Task.yield()
        }
        XCTFail("Shared feed work did not settle", file: file, line: line)
    }
}
