import XCTest
@testable import Podcastr

// MARK: - UserIdentityWiringTests
//
// Table-driven coverage for the wiring contract in
// `docs/spec/briefs/identity-05-synthesis.md` §5.3 — the canonical "what
// signs vs. what stays local" matrix. Every row owned by Slice B is
// asserted here against a recording mock signer, so a regression that
// silently drops a publish (or, worse, signs an agent-authored artefact
// with the user's identity) trips a test instead of leaking out a relay.
//
// The publish leg is intentionally NOT exercised — the production calls
// hit `FeedbackRelayClient.publish(...)` over a real WebSocket which will
// time out under XCTest. We assert on what the recording signer captured
// before the network leg, which is the load-bearing part of the contract.

@MainActor
final class UserIdentityWiringTests: XCTestCase {

    private var storeFileURL: URL!
    private var store: AppStateStore!
    private var signer: RecordingSigner!
    private var identity: UserIdentityStore!

    override func setUp() async throws {
        try await super.setUp()
        let made = await AppStateTestSupport.makeIsolatedStore()
        storeFileURL = made.fileURL
        store = made.store
        signer = RecordingSigner()
        identity = UserIdentityStore.shared
        identity._setSignerForTesting(signer)
    }

    override func tearDown() async throws {
        identity._clearSignerForTesting()
        if let storeFileURL {
            AppStateTestSupport.disposeIsolatedStore(at: storeFileURL)
        }
        store = nil
        storeFileURL = nil
        signer = nil
        identity = nil
        try await super.tearDown()
    }

    // MARK: - §5.3 row: Profile (kind:0, signs)

    func testPublishProfileSignsKindZero() async throws {
        // Publish through the network leg will fail (no relay) — that's
        // expected. The contract we care about is "did the signer see a
        // kind-0 sign call with the right payload?"
        _ = try? await identity.publishProfile(
            name: "alice-test",
            displayName: "Alice",
            about: "Hello",
            picture: "https://example.test/a.png"
        )

        let signed = try XCTUnwrap(signer.calls.first, "Expected one sign call for kind:0 profile.")
        XCTAssertEqual(signed.kind, 0, "Profile must sign as kind 0.")
        XCTAssertTrue(signed.content.contains("\"name\":\"alice-test\""), "Content must include name.")
        XCTAssertTrue(signed.content.contains("\"display_name\":\"Alice\""), "Content must include display_name.")
        XCTAssertTrue(signed.content.contains("\"about\":\"Hello\""), "Content must include about.")
        XCTAssertTrue(signed.content.contains("\"picture\":\"https:\\/\\/example.test\\/a.png\"")
                      || signed.content.contains("\"picture\":\"https://example.test/a.png\""),
                      "Content must include picture URL.")
    }

    // MARK: - §5.3 row: Notes (user) — signs kind 1

    func testAddNoteUserAuthorSignsKindOne() async throws {
        // The default `addNote(text:kind:target:)` overload is the user
        // path — it should fire `publishUserNote` from a fire-and-forget
        // Task. Wait for the Task to drain.
        _ = store.addNote(text: "first user note", kind: .free)
        try await waitForSignerCalls(count: 1)

        let signed = try XCTUnwrap(signer.calls.first)
        XCTAssertEqual(signed.kind, 1, "User notes must sign as kind 1.")
        XCTAssertEqual(signed.content, "first user note")
        XCTAssertTrue(signed.tags.contains(["t", "note"]), "User notes must carry [\"t\", \"note\"] tag.")
    }

    func testAddNoteExplicitUserAuthorSignsKindOne() async throws {
        _ = store.addNote(text: "explicit user note", kind: .free, target: nil, author: .user)
        try await waitForSignerCalls(count: 1)

        let signed = try XCTUnwrap(signer.calls.first)
        XCTAssertEqual(signed.kind, 1)
        XCTAssertEqual(signed.content, "explicit user note")
    }

    // MARK: - §5.3 row: Notes (agent tool) — does NOT sign

    func testAddNoteAgentAuthorDoesNotSign() async throws {
        // Agent-authored notes append locally only.
        _ = store.addNote(text: "agent note", kind: .free, target: nil, author: .agent)
        // Give any (stray) fire-and-forget Task a chance to land.
        try await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertTrue(signer.calls.isEmpty, "Agent-authored notes must not reach the user signer.")
    }

    func testAgentToolCreateNoteDoesNotSign() async throws {
        // The createNote agent tool path is the canonical "did Slice B
        // wire `author: .agent` at the call-site?" check.
        _ = AgentTools.dispatchNotesMemory(
            name: AgentTools.Names.createNote,
            args: ["text": "agent tool note", "kind": "free"],
            store: store,
            batchID: UUID()
        )
        try await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertTrue(signer.calls.isEmpty, "AgentTools.createNote must not reach the user signer.")
        // The note still landed locally.
        XCTAssertEqual(store.state.notes.last?.text, "agent tool note")
        XCTAssertEqual(store.state.notes.last?.author, .agent)
    }

    // MARK: - §5.3 row: Memories — does NOT sign

    func testAddAgentMemoryDoesNotSign() async throws {
        _ = store.addAgentMemory(content: "long-running fact")
        try await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertTrue(signer.calls.isEmpty, "Memories must not reach the user signer.")
    }

    // MARK: - §5.3 row: Clips, source ≠ .agent — signs kind 9802

    func testAddClipTouchSourceSignsKindNineEightZeroTwo() async throws {
        let sub = UUID()
        let ep = UUID()
        let clip = Clip(
            episodeID: ep,
            subscriptionID: sub,
            startMs: 1_000,
            endMs: 5_000,
            caption: "Worth re-listening",
            transcriptText: "the prose at the heart of the clip",
            source: .touch
        )
        store.addClip(clip)
        try await waitForSignerCalls(count: 1)

        let signed = try XCTUnwrap(signer.calls.first)
        XCTAssertEqual(signed.kind, 9802, "Clips must sign as NIP-84 kind 9802.")
        XCTAssertEqual(signed.content, "the prose at the heart of the clip")
        XCTAssertTrue(signed.tags.contains(["context", "the prose at the heart of the clip"]),
                      "Clip must carry the [\"context\", transcript] tag.")
        XCTAssertTrue(signed.tags.contains(["alt", "Worth re-listening"]),
                      "Clip with caption must carry the [\"alt\", caption] tag.")
    }

    func testAddClipAutoSourceSignsKindNineEightZeroTwo() async throws {
        let clip = Clip(
            episodeID: UUID(),
            subscriptionID: UUID(),
            startMs: 0,
            endMs: 1_000,
            transcriptText: "auto-snip text",
            source: .auto
        )
        store.addClip(clip)
        try await waitForSignerCalls(count: 1)

        XCTAssertEqual(signer.calls.first?.kind, 9802)
    }

    func testAddClipConvenienceOverloadSignsForNonAgentSource() async throws {
        // The convenience builder routes through `addClip(_:)` — the
        // publish wiring must fire the same way.
        _ = store.addClip(
            episodeID: UUID(),
            subscriptionID: UUID(),
            startMs: 0,
            endMs: 2_000,
            transcriptText: "auto-snip via convenience",
            source: .headphone
        )
        try await waitForSignerCalls(count: 1)

        XCTAssertEqual(signer.calls.first?.kind, 9802)
    }

    // MARK: - §5.3 row: Clips, source == .agent — does NOT sign

    func testAddClipAgentSourceDoesNotSign() async throws {
        let clip = Clip(
            episodeID: UUID(),
            subscriptionID: UUID(),
            startMs: 0,
            endMs: 1_000,
            transcriptText: "agent-captured snippet",
            source: .agent
        )
        store.addClip(clip)
        try await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertTrue(signer.calls.isEmpty, "Agent-sourced clips must not reach the user signer.")
    }

    // MARK: - Note.author Codable backward-compat

    func testNoteDecodesLegacyJSONWithoutAuthorAsUser() throws {
        // Pre-NoteAuthor snapshot: no `author` key. Must default to `.user`.
        let legacyJSON = #"""
        {
          "id": "11111111-1111-1111-1111-111111111111",
          "text": "legacy note",
          "kind": "free",
          "createdAt": 0,
          "deleted": false
        }
        """#.data(using: .utf8)!

        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .secondsSince1970
        let note = try decoder.decode(Note.self, from: legacyJSON)
        XCTAssertEqual(note.author, .user, "Legacy notes (no `author` field) must default to `.user`.")
        XCTAssertEqual(note.text, "legacy note")
    }

    func testNoteRoundTripsAgentAuthor() throws {
        let original = Note(text: "agent-recorded", kind: .free, target: nil, author: .agent)
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(Note.self, from: data)
        XCTAssertEqual(decoded.author, .agent, "Encoded `.agent` must round-trip.")
        XCTAssertEqual(decoded.text, "agent-recorded")
    }

    func testNoteRoundTripsUserAuthor() throws {
        let original = Note(text: "user-recorded", kind: .free, target: nil, author: .user)
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(Note.self, from: data)
        XCTAssertEqual(decoded.author, .user, "Encoded `.user` must round-trip.")
    }

    // MARK: - Helpers

    /// Polls until the recording signer has captured at least `count`
    /// calls, or fails after a generous timeout. Wiring-layer Tasks are
    /// fire-and-forget — there's no completion handle to await — so a
    /// short polling loop is the cleanest seam.
    private func waitForSignerCalls(
        count: Int,
        timeout: TimeInterval = 2.0,
        file: StaticString = #file,
        line: UInt = #line
    ) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while signer.calls.count < count {
            if Date() > deadline {
                XCTFail(
                    "Timed out waiting for \(count) signer call(s); got \(signer.calls.count).",
                    file: file, line: line
                )
                return
            }
            try await Task.sleep(nanoseconds: 25_000_000)
        }
    }
}

// MARK: - RecordingSigner

/// Test double for `NostrSigner` that records every `sign(_:)` call so the
/// wiring tests can assert which call-sites reached the signer (and which
/// didn't). The returned `SignedNostrEvent` is a stub — the publish leg
/// is not exercised in these tests; production hits a real WebSocket.
final class RecordingSigner: NostrSigner, @unchecked Sendable {
    struct Call: Sendable {
        let kind: Int
        let content: String
        let tags: [[String]]
    }

    private let queue = DispatchQueue(label: "RecordingSigner")
    private var _calls: [Call] = []

    var calls: [Call] {
        queue.sync { _calls }
    }

    func publicKey() async throws -> String {
        String(repeating: "0", count: 64)
    }

    func sign(_ draft: NostrEventDraft) async throws -> SignedNostrEvent {
        queue.sync {
            _calls.append(Call(kind: draft.kind, content: draft.content, tags: draft.tags))
        }
        return SignedNostrEvent(
            id: String(repeating: "a", count: 64),
            pubkey: String(repeating: "0", count: 64),
            created_at: draft.createdAt,
            kind: draft.kind,
            tags: draft.tags,
            content: draft.content,
            sig: String(repeating: "b", count: 128)
        )
    }
}
