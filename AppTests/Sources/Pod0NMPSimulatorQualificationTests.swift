import Foundation
import XCTest
@testable import Podcastr

#if canImport(NMP) && targetEnvironment(simulator)
import NMP

final class Pod0NMPSimulatorQualificationTests: XCTestCase {
    func testHostnameRelayInformationBoundedQueryDurableWriteAndCleanCancellation() async throws {
        let relay = try Pod0ControlledRelayHarness()
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("pod0-nmp-simulator-\(UUID().uuidString)", isDirectory: true)
        let layout = Pod0NMPStoreLayout(rootDirectory: root)
        let configuration = Pod0NMPConfiguration(
            storeURL: layout.storeURL,
            indexerRelays: [],
            operatorRelay: relay.relayURL,
            fallbackRelays: [],
            allowedLocalRelayHosts: ["localhost"],
            limits: .init(maxRelays: 1, maxNativeTasks: 4, maxAuthCapabilities: 1)
        )
        var composition: Pod0NMPComposition? = try Pod0NMPComposition(
            configuration: configuration,
            layout: layout
        )
        defer {
            composition?.shutdown()
            relay.stop()
            try? FileManager.default.removeItem(at: root)
        }
        let engine = try XCTUnwrap(composition?.engine)

        XCTAssertEqual(URL(string: relay.relayURL)?.host, "localhost")
        XCTAssertEqual(configuration.allowedLocalRelayHosts, ["localhost"])
        let productionDefault = Pod0NMPConfiguration(
            storeURL: root.appendingPathComponent("default.redb"),
            indexerRelays: [],
            operatorRelay: nil,
            fallbackRelays: []
        )
        XCTAssertEqual(productionDefault.allowedLocalRelayHosts, [])

        let relayInformation = try await engine.relayInformation(
            for: relay.relayURL,
            policy: .refresh
        )
        XCTAssertTrue(isHarnessRelay(relayInformation.relay, harnessURL: relay.relayURL))
        XCTAssertEqual(relayInformation.document.name, "Pod0 Simulator Relay")
        XCTAssertEqual(relayInformation.document.supportedNips, [1, 11])
        XCTAssertEqual(relayInformation.freshness, .fresh)
        XCTAssertNil(relayInformation.lastError)
        XCTAssertEqual(relayInformation.documentRevision.count, 64)
        XCTAssertEqual(relay.snapshot().nip11Requests, 1)

        let secretKey = String(repeating: "0", count: 63) + "1"
        let account = try await engine.addAccount(secretKey: secretKey)
        try engine.setActiveAccount(account.publicKey)

        let query = try engine.observe(
            NMPFilter(
                kinds: [1],
                authors: .literal([account.publicKey]),
                tags: [:]
            ),
            window: .expandable(initial: 1, max: 1)
        )
        let acquired = await firstBatch(from: query, timeoutSeconds: 8) { batch in
            batch.evidence.sources.contains {
                isHarnessRelay($0.relay, harnessURL: relay.relayURL) && $0.reconciledThrough != nil
            }
        }
        let acquiredBatch = try XCTUnwrap(acquired, "hostname relay must reconcile the bounded query")
        XCTAssertLessThanOrEqual(acquiredBatch.rows.count, 1)
        XCTAssertEqual(acquiredBatch.load, .idle)
        XCTAssertTrue(acquiredBatch.evidence.shortfall.isEmpty)
        let subscriptionID = try XCTUnwrap(relay.snapshot().requestSubscriptionIDs.first)

        let receipt = try await engine.publish(
            WriteIntent(
                payload: .unsigned(
                    pubkey: account.publicKey,
                    createdAt: UInt64(Date().timeIntervalSince1970),
                    kind: 1,
                    tags: [],
                    content: "Pod0 simulator qualification"
                ),
                durability: .durable,
                routing: .authorOutbox,
                identityOverride: account.publicKey
            )
        )
        let statuses = await receiptStatuses(
            from: receipt,
            harnessURL: relay.relayURL,
            timeoutSeconds: 10
        )
        XCTAssertTrue(statuses.contains(.accepted))
        let eventID = try XCTUnwrap(statuses.compactMap { status -> String? in
            if case .signed(let eventID) = status { return eventID }
            return nil
        }.first)
        XCTAssertTrue(statuses.contains { status in
            if case .routed(let relays) = status {
                return relays.contains { isHarnessRelay($0, harnessURL: relay.relayURL) }
            }
            return false
        })
        XCTAssertTrue(statuses.contains { status in
            if case .sent(let relayURL, _, _) = status {
                return isHarnessRelay(relayURL, harnessURL: relay.relayURL)
            }
            return false
        })
        XCTAssertTrue(statuses.contains { status in
            if case .acked(let relayURL) = status {
                return isHarnessRelay(relayURL, harnessURL: relay.relayURL)
            }
            return false
        })
        XCTAssertEqual(relay.snapshot().acceptedEventIDs, [eventID])

        let delivered = await firstBatch(from: query, timeoutSeconds: 8) { batch in
            batch.rows.contains { $0.id == eventID && $0.sources.contains {
                isHarnessRelay($0, harnessURL: relay.relayURL)
            }}
        }
        let deliveredBatch = try XCTUnwrap(delivered, "relay echo must reach the public query")
        XCTAssertEqual(deliveredBatch.rows.map(\.id), [eventID])
        XCTAssertLessThanOrEqual(deliveredBatch.rows.count, 1)

        try query.requestRows(atLeast: 2)
        let atBound = await firstBatch(from: query, timeoutSeconds: 5) { $0.load == .atBound(max: 1) }
        XCTAssertNotNil(atBound, "the public query must report its declared delivery bound")

        query.cancel()
        let cancelled = await waitForRelay(relay, timeoutSeconds: 5) {
            $0.closedSubscriptionIDs.contains(subscriptionID)
        }
        XCTAssertNotNil(cancelled, "query cancellation must send CLOSE to the controlled relay")
        XCTAssertTrue(try engine.removeAccount(account))

        composition?.shutdown()
        let tornDown = await waitForRelay(relay, timeoutSeconds: 5) { $0.activeWebSockets == 0 }
        XCTAssertNotNil(tornDown, "engine shutdown must close the relay transport")
        try composition?.resetStoreAfterShutdown()
        composition = nil
    }

    private func firstBatch(
        from query: NMPQuery,
        timeoutSeconds: UInt64,
        matching predicate: @escaping @Sendable (RowBatch) -> Bool
    ) async -> RowBatch? {
        await withTaskGroup(of: RowBatch?.self) { group in
            group.addTask {
                for await batch in query where predicate(batch) { return batch }
                return nil
            }
            group.addTask {
                try? await Task.sleep(nanoseconds: timeoutSeconds * 1_000_000_000)
                return nil
            }
            let result = await group.next() ?? nil
            group.cancelAll()
            return result
        }
    }

    private func receiptStatuses(
        from receipt: Receipt,
        harnessURL: String,
        timeoutSeconds: UInt64
    ) async -> [WriteStatus] {
        await withTaskGroup(of: [WriteStatus].self) { group in
            group.addTask {
                var statuses: [WriteStatus] = []
                for await status in receipt.status {
                    statuses.append(status)
                    if case .acked(let relayURL) = status,
                       isHarnessRelay(relayURL, harnessURL: harnessURL) { break }
                }
                return statuses
            }
            group.addTask {
                try? await Task.sleep(nanoseconds: timeoutSeconds * 1_000_000_000)
                return []
            }
            let result = await group.next() ?? []
            group.cancelAll()
            return result
        }
    }

    private func waitForRelay(
        _ relay: Pod0ControlledRelayHarness,
        timeoutSeconds: UInt64,
        matching predicate: @escaping @Sendable (Pod0ControlledRelayHarness.Snapshot) -> Bool
    ) async -> Pod0ControlledRelayHarness.Snapshot? {
        let deadline = ContinuousClock.now + .seconds(Int64(timeoutSeconds))
        while ContinuousClock.now < deadline {
            let snapshot = relay.snapshot()
            if predicate(snapshot) { return snapshot }
            try? await Task.sleep(for: .milliseconds(20))
        }
        return nil
    }
}

private func isHarnessRelay(_ candidate: String, harnessURL: String) -> Bool {
    guard let candidate = URL(string: candidate), let harness = URL(string: harnessURL) else {
        return false
    }
    return candidate.scheme == harness.scheme && candidate.host == harness.host && candidate.port == harness.port
}
#endif
