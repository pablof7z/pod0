import XCTest
@testable import Podcastr

final class NostrIntentWireTests: XCTestCase {
    func testIntentRequestEncodesRustShape() throws {
        let request = NostrIntentRequest(
            input: "alice@example.com",
            scopes: [.nostrRef, .nip50Profiles],
            textTargets: .userPreferred
        )
        let data = try XCTUnwrap(request.jsonString()?.data(using: .utf8))
        let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]

        XCTAssertEqual(object?["input"] as? String, "alice@example.com")
        XCTAssertEqual(object?["text_targets"] as? String, "UserPreferred")
        let scopes = object?["scopes"] as? [[String: String]]
        XCTAssertEqual(scopes?[0], ["namespace": "nostr", "name": "ref"])
        XCTAssertEqual(scopes?[1], ["namespace": "nip50", "name": "profiles"])
    }

    func testClassificationDecodesCandidatesAndTargets() throws {
        let envelope = try XCTUnwrap(NostrIntentClassificationEnvelope.decode(json: """
        {"ok":true,"classification":{"Candidates":[{"scope":{"namespace":"nip50","name":"profiles"},"target":{"Nip05":{"identifier":"alice@example.com"}}}]}}
        """))

        guard case .candidates(let candidates) = envelope.classification else {
            return XCTFail("expected candidates")
        }
        XCTAssertEqual(candidates.first?.scope, .nip50Profiles)
        XCTAssertEqual(candidates.first?.target, .nip05(identifier: "alice@example.com"))
    }

    func testClassificationDecodesSecretLikeWithoutEchoingInput() throws {
        let envelope = try XCTUnwrap(NostrIntentClassificationEnvelope.decode(json: """
        {"ok":true,"classification":{"Rejection":"SecretLike"}}
        """))

        XCTAssertEqual(envelope.classification, .rejection(.secretLike))
    }

    func testDispatchOutcomeDecodesDirectRefAndTextQuery() throws {
        let direct = try XCTUnwrap(NostrIntentDispatchOutcome.decode(json: """
        {"ok":true,"dispatched":{"scope":{"namespace":"nostr","name":"ref"},"target":{"DirectRef":{"uri":"nostr:npub1abc"}}}}
        """))
        XCTAssertEqual(direct, .dispatched(.directRef(uri: "nostr:npub1abc")))

        let nip05 = try XCTUnwrap(NostrIntentDispatchOutcome.decode(json: """
        {"ok":true,"dispatched":{"scope":{"namespace":"nip50","name":"profiles"},"target":{"Nip05":{"identifier":"alice@example.com"}}}}
        """))
        XCTAssertEqual(nip05, .dispatched(.nip05(identifier: "alice@example.com")))

        let text = try XCTUnwrap(NostrIntentDispatchOutcome.decode(json: """
        {"ok":true,"dispatched":{"scope":{"namespace":"nip50","name":"profiles"},"target":{"TextQuery":{"request_json":"{}"}}}}
        """))
        XCTAssertEqual(text, .dispatched(.textQuery(requestJSON: "{}")))
    }

    func testResolvedProfileSelectionReturnsDeterministicNewPubkey() {
        let profiles = [
            "ff": ResolvedProfile(displayName: "Later", pictureUrl: nil),
            "aa": ResolvedProfile(displayName: nil, pictureUrl: nil),
            "bb": ResolvedProfile(displayName: "Existing", pictureUrl: nil)
        ]

        let selected = NostrResolvedProfileSelection.firstNewPubkey(
            in: profiles,
            excluding: ["bb"])

        XCTAssertEqual(selected, "aa")
    }

    func testResolvedProfileSelectionReturnsNilWhenOnlyExistingRowsRemain() {
        let profiles = [
            "aa": ResolvedProfile(displayName: nil, pictureUrl: nil),
            "bb": ResolvedProfile(displayName: "Existing", pictureUrl: nil)
        ]

        let selected = NostrResolvedProfileSelection.firstNewPubkey(
            in: profiles,
            excluding: ["aa", "bb"])

        XCTAssertNil(selected)
    }

    func testNostrSearchProjectionDecodesSessionHits() throws {
        let data = Data("""
        {"t":"snapshot","v":{"projections":{"nmp.nip50.search.ios-1":{"hits":[{"id":"e1","author":"a1","kind":0,"created_at":1700000001,"content":"{\\"display_name\\":\\"Alice\\",\\"about\\":\\"Builder\\"}","tags":[],"relay_provenance":["wss://relay.example/"],"source":{"Relay":"wss://relay.example/"}}]}}}}
        """.utf8)

        let sessions = NostrSearchProjection.decodeSessions(from: data)
        let hit = try XCTUnwrap(sessions["ios-1"]?.hits.first)
        XCTAssertEqual(hit.displayName, "Alice")
        XCTAssertEqual(hit.detail, "Builder")
        XCTAssertEqual(hit.source, .relay("wss://relay.example/"))
    }

    func testDecodedNostrRefTargetDecodesProfilesEventsAndAddresses() throws {
        XCTAssertEqual(
            DecodedNostrRefTarget.decode(json: #"{"ok":true,"target":"profile","pubkey":"abc"}"#),
            .profile(pubkey: "abc")
        )
        XCTAssertEqual(
            DecodedNostrRefTarget.decode(json: #"{"ok":true,"target":"event","event_id":"evt"}"#),
            .event(eventID: "evt")
        )
        XCTAssertEqual(
            DecodedNostrRefTarget.decode(json: #"{"ok":true,"target":"address","pubkey":"def"}"#),
            .address(pubkey: "def")
        )
    }
}
