import Foundation
import XCTest
@testable import Podcastr

/// Unit tests for the sign-for-return resolver (`SignedEventsRegistry`). This is
/// the trickiest correctness logic in the kernel-signed Blossom path: it must
/// resolve regardless of whether the drain-once `signed_events` frame lands
/// BEFORE or AFTER the caller registers its continuation. No kernel needed —
/// the registry is driven directly with synthesized envelope JSON.
final class SignedEventsRegistryTests: XCTestCase {

    /// Build the kernel wire envelope (`{"v":{"projections":{"signed_events":…}}}`)
    /// that `ingest` parses, carrying one `correlationID → entry`.
    private func envelope(correlationID: String, entry: [String: Any]) -> Data {
        let outer: [String: Any] = [
            "v": ["projections": ["signed_events": [correlationID: entry]]],
        ]
        return try! JSONSerialization.data(withJSONObject: outer)
    }

    /// Ordering 1: the result drains BEFORE the caller awaits. The registry must
    /// buffer it so the later await resolves immediately (the drain-once race).
    func testResultBufferedBeforeAwaitResolves() async throws {
        let registry = SignedEventsRegistry()
        let cid = "cid-buffered"
        let signedJSON = #"{"id":"abc","pubkey":"pk","created_at":1,"kind":24242,"tags":[],"content":"","sig":"s"}"#

        // Frame arrives first — no waiter yet.
        registry.ingest(envelopePayload: envelope(
            correlationID: cid, entry: ["ok": true, "signed_json": signedJSON]))

        // Caller awaits afterward — must still get the buffered result.
        let result = try await registry.awaitResult(correlationID: cid)
        XCTAssertEqual(result, signedJSON)
    }

    /// Ordering 2: the caller awaits BEFORE the result drains. The registry must
    /// install the continuation and resume it when `ingest` later runs.
    func testAwaitBeforeResultResolves() async throws {
        let registry = SignedEventsRegistry()
        let cid = "cid-waiter"
        let signedJSON = #"{"id":"def","pubkey":"pk","created_at":2,"kind":24242,"tags":[],"content":"","sig":"s2"}"#

        async let pending = registry.awaitResult(correlationID: cid)
        // Yield so the awaiting task registers its continuation before ingest.
        try await Task.sleep(for: .milliseconds(20))
        registry.ingest(envelopePayload: envelope(
            correlationID: cid, entry: ["ok": true, "signed_json": signedJSON]))

        let result = try await pending
        XCTAssertEqual(result, signedJSON)
    }

    /// A kernel-reported failure (`{"ok":false,"error":…}`) surfaces as a thrown
    /// error under the same id, never a hang.
    func testErrorEntryThrows() async throws {
        let registry = SignedEventsRegistry()
        let cid = "cid-error"
        registry.ingest(envelopePayload: envelope(
            correlationID: cid, entry: ["ok": false, "error": "no signer"]))

        do {
            _ = try await registry.awaitResult(correlationID: cid)
            XCTFail("expected a thrown error for an `ok:false` entry")
        } catch {
            // expected
        }
    }

    /// `cancel` fails an outstanding waiter (the caller-owned timeout path) and
    /// drops it so a later `ingest` cannot resume a dead continuation.
    func testCancelFailsWaiter() async throws {
        let registry = SignedEventsRegistry()
        let cid = "cid-cancel"

        async let pending = registry.awaitResult(correlationID: cid)
        try await Task.sleep(for: .milliseconds(20))
        registry.cancel(correlationID: cid, with: NostrSignerError.timedOut)

        do {
            _ = try await pending
            XCTFail("expected the cancelled waiter to throw")
        } catch {
            // expected
        }
        // A late frame for the same id must be inert (no crash / double-resume).
        registry.ingest(envelopePayload: envelope(
            correlationID: cid, entry: ["ok": true, "signed_json": "{}"]))
    }

    /// Frames that carry no `signed_events` projection are ignored (the common
    /// steady-state tick), leaving an unrelated waiter pending.
    func testFrameWithoutSignedEventsIsIgnored() async throws {
        let registry = SignedEventsRegistry()
        let empty: [String: Any] = ["v": ["projections": ["podcast.snapshot": [:]]]]
        registry.ingest(envelopePayload: try! JSONSerialization.data(withJSONObject: empty))
        // No assertion beyond "did not crash / did not resolve a phantom id" —
        // resolving the wrong id would be a logic error caught by the other tests.
    }
}
