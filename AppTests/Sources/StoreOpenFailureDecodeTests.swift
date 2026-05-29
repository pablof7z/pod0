import XCTest
@testable import Podcastr

/// Coverage for `KernelUpdateResult.extractStoreOpenFailure(envelopePayload:)` —
/// the raw second-pass read that surfaces the mandatory NMP v0.1.0 `store_open_failure`
/// diagnostic (V-67). The field rides the generic kernel snapshot (sibling of
/// `projections`) which the typed `PodcastUpdate` decode intentionally drops, so
/// it must be read straight from the wire envelope.
final class StoreOpenFailureDecodeTests: XCTestCase {

    func testExtractsFailureReasonFromEnvelope() {
        let json = """
        {"t":"snapshot","v":{"running":true,"rev":1,"schema_version":1,\
        "store_open_failure":"lmdb open failed: MDB_PANIC: Update of meta page failed"}}
        """.data(using: .utf8)!

        let reason = KernelUpdateResult.extractStoreOpenFailure(envelopePayload: json)

        XCTAssertEqual(reason, "lmdb open failed: MDB_PANIC: Update of meta page failed")
    }

    func testHealthySessionHasNoFailure() {
        // The kernel omits the key entirely in healthy sessions
        // (`skip_serializing_if = Option::is_none`).
        let json = """
        {"t":"snapshot","v":{"running":true,"rev":7,"schema_version":1,\
        "projections":{"active_account":"npub1abc"}}}
        """.data(using: .utf8)!

        XCTAssertNil(KernelUpdateResult.extractStoreOpenFailure(envelopePayload: json))
    }

    func testMalformedPayloadDegradesToNil() {
        let json = Data("not json at all".utf8)
        XCTAssertNil(KernelUpdateResult.extractStoreOpenFailure(envelopePayload: json))
    }

    func testMissingEnvelopeValueDegradesToNil() {
        let json = Data(#"{"t":"snapshot"}"#.utf8)
        XCTAssertNil(KernelUpdateResult.extractStoreOpenFailure(envelopePayload: json))
    }
}
