import XCTest
@testable import Podcastr

// ─── iOS snake_case decode guard for `podcast.social` ─────────────────────────
//
// iOS is the platform that FROZE in #371 when an embedded DTO carried explicit
// snake_case CodingKeys (the bridge uses `.convertFromSnakeCase`, so explicit
// snake_case keys → keyNotFound → ALL frames dropped → UI freeze). Android has
// a round-trip wire test (DomainFrameWireTest.kt); this is the matching iOS
// guard for the new `podcast.social` domain.
//
// Each fixture is Rust-shaped: snake_case field names exactly as
// `nmp_app_podcast_decode_update_frame` injects them under
// `v.projections["podcast.social"]`. A `@SerialName`/CodingKeys regression on
// `NostrConversationDTO` / `NostrConversationTurnDTO` would make these fail.

private func makeEnvelope(projections: [String: Any]) -> Data {
    let body: [String: Any] = [
        "t": "snapshot",
        "v": ["projections": projections]
    ]
    return try! JSONSerialization.data(withJSONObject: body)
}

/// A populated `podcast.social` projection: one conversation, two turns
/// (inbound + outbound), all wire keys snake_case.
private func socialProjection(rev: Int) -> [String: Any] {
    [
        "rev": rev,
        // social = null is the tombstone shape for the follow-graph slice;
        // a populated conversation list can still ride alongside it.
        "social": NSNull(),
        "nostr_conversations": [
            [
                "root_event_id": "deadbeef001",
                "counterparty_hex": "aabbccdd001",
                "participants": ["aabbccdd001", "11223344001"],
                "trusted": true,
                // Explicit per-peer flags — always present on the wire (serde
                // serializes all struct fields). trusted=true via explicit
                // approval here: peer_approved=true, peer_blocked=false.
                "peer_approved": true,
                "peer_blocked": false,
                "first_seen": 1_717_200_000,
                "last_activity": 1_717_286_400,
                "turns": [
                    [
                        "event_id": "evt-001",
                        "direction": "inbound",
                        "pubkey_hex": "aabbccdd001",
                        "created_at": 1_717_200_001,
                        "content": "Hello from Nostr"
                    ],
                    [
                        "event_id": "evt-002",
                        "direction": "outbound",
                        "pubkey_hex": "11223344001",
                        "created_at": 1_717_200_100,
                        "content": "Reply from agent"
                    ]
                ]
            ]
        ]
    ]
}

final class NostrConversationSocialDomainTests: XCTestCase {

    /// The `podcast.social` sidecar decodes through the SAME bridge seam
    /// (`PodcastDomainFrames.decode`, `.convertFromSnakeCase`) and the wire DTO
    /// fields survive the snake_case → camelCase conversion.
    func testSocialDomainFrameDecodesSnakeCaseNostrConversations() throws {
        let data = makeEnvelope(projections: [
            DomainSchema.social: socialProjection(rev: 9)
        ])
        let frames = try XCTUnwrap(
            PodcastDomainFrames.decode(from: data),
            "frame with podcast.social sidecar must yield a non-nil PodcastDomainFrames")
        let soc = try XCTUnwrap(frames.social, "social domain must be non-nil")
        XCTAssertEqual(soc.rev, 9)

        // social = null → tombstone for the follow-graph slice.
        XCTAssertNil(soc.social, "social snapshot must decode as nil (tombstone)")

        let convos = try XCTUnwrap(soc.nostrConversations, "nostr_conversations must decode")
        XCTAssertEqual(convos.count, 1)

        let convo = convos[0]
        // root_event_id → rootEventId (lowercase d, .convertFromSnakeCase)
        XCTAssertEqual(convo.rootEventId, "deadbeef001")
        // counterparty_hex → counterpartyHex
        XCTAssertEqual(convo.counterpartyHex, "aabbccdd001")
        XCTAssertTrue(convo.trusted)
        // peer_approved → peerApproved, peer_blocked → peerBlocked (.convertFromSnakeCase)
        XCTAssertTrue(convo.peerApproved, "peer_approved must decode to peerApproved")
        XCTAssertFalse(convo.peerBlocked, "peer_blocked must decode to peerBlocked")
        XCTAssertEqual(convo.firstSeen, 1_717_200_000)   // first_seen
        XCTAssertEqual(convo.lastActivity, 1_717_286_400) // last_activity
        XCTAssertEqual(convo.participants.count, 2)

        XCTAssertEqual(convo.turns.count, 2)
        let inbound = convo.turns[0]
        XCTAssertEqual(inbound.eventId, "evt-001")        // event_id
        XCTAssertEqual(inbound.direction, "inbound")
        XCTAssertEqual(inbound.pubkeyHex, "aabbccdd001")  // pubkey_hex
        XCTAssertEqual(inbound.createdAt, 1_717_200_001)  // created_at
        XCTAssertEqual(inbound.content, "Hello from Nostr")

        let outbound = convo.turns[1]
        XCTAssertEqual(outbound.eventId, "evt-002")
        XCTAssertEqual(outbound.direction, "outbound")
        XCTAssertEqual(outbound.content, "Reply from agent")

        // Misc domain (which previously carried social/agentNotes) is absent.
        XCTAssertNil(frames.misc, "misc domain must be absent in a social-only frame")
    }

    /// The wire DTO → domain mapping (`KernelModel.nostrConversationFromDTO`) is
    /// the exact code the merge path uses to populate `AppState.nostrConversations`.
    /// Asserts the field/timestamp/direction translation contract:
    ///   rootEventId  → rootEventID (uppercase)
    ///   Int unix     → Date
    ///   "inbound"    → .incoming, "outbound" → .outgoing
    ///
    /// `@MainActor`: `KernelModel.nostrConversationFromDTO` is main-actor
    /// isolated (KernelModel is `@MainActor`), so the synchronous call must run
    /// on the main actor.
    @MainActor
    func testNostrConversationDTOMapsToDomainRecord() throws {
        let data = makeEnvelope(projections: [
            DomainSchema.social: socialProjection(rev: 9)
        ])
        let frames = try XCTUnwrap(PodcastDomainFrames.decode(from: data))
        let dto = try XCTUnwrap(frames.social?.nostrConversations?.first)

        let record = KernelModel.nostrConversationFromDTO(dto)

        // rootEventId → rootEventID (uppercase ID), counterparty mapping.
        XCTAssertEqual(record.rootEventID, "deadbeef001")
        XCTAssertEqual(record.counterpartyPubkey, "aabbccdd001")
        // Int unix timestamps → Date.
        XCTAssertEqual(record.firstSeen, Date(timeIntervalSince1970: 1_717_200_000))
        XCTAssertEqual(record.lastTouched, Date(timeIntervalSince1970: 1_717_286_400))

        XCTAssertEqual(record.turns.count, 2)
        // "inbound" → .incoming
        XCTAssertEqual(record.turns[0].direction, .incoming)
        XCTAssertEqual(record.turns[0].eventID, "evt-001")
        XCTAssertEqual(record.turns[0].pubkey, "aabbccdd001")
        XCTAssertEqual(record.turns[0].createdAt, Date(timeIntervalSince1970: 1_717_200_001))
        XCTAssertEqual(record.turns[0].content, "Hello from Nostr")
        // "outbound" → .outgoing
        XCTAssertEqual(record.turns[1].direction, .outgoing)
        XCTAssertEqual(record.turns[1].eventID, "evt-002")
    }

    /// A frame whose social payload omits the optional `nostr_conversations`
    /// key still decodes (forward/back compat) — the array is simply nil.
    func testSocialDomainFrameWithoutConversationsDecodes() throws {
        let data = makeEnvelope(projections: [
            DomainSchema.social: [
                "rev": 4,
                "social": NSNull()
            ] as [String: Any]
        ])
        let frames = try XCTUnwrap(PodcastDomainFrames.decode(from: data))
        let soc = try XCTUnwrap(frames.social)
        XCTAssertEqual(soc.rev, 4)
        XCTAssertNil(soc.nostrConversations, "absent nostr_conversations must decode as nil, not error")
    }
}
