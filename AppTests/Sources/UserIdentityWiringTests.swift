import XCTest
@testable import Podcastr

// MARK: - UserIdentityWiringTests
//
// Table-driven coverage for the wiring contract in
// `docs/spec/briefs/identity-05-synthesis.md` §5.3 — the canonical "what
// signs vs. what stays local" matrix. Every row owned by Slice B is
// asserted here, so a regression that silently drops a publish (or, worse,
// signs an agent-authored artefact with the user's identity) trips a test
// instead of leaking out a relay.
//
// kind:0/1/9802 user-content signing now lives in the Rust kernel: a
// `.localKey` identity dispatches `podcast.social.*` to the kernel rather
// than signing through the Swift `NostrSigner`. So the "signs" rows assert
// against a recording KERNEL seam (`_setKernelRecorderForTesting`), and the
// "does NOT sign" rows assert NO kernel dispatch reached the seam. The
// publish/relay leg is owned by the kernel and not exercised here.

@MainActor
final class UserIdentityWiringTests: XCTestCase {

    private var storeFileURL: URL!
    private var store: AppStateStore!
    private var signer: RecordingSigner!
    private var identity: UserIdentityStore!
    private var kernelDispatches: KernelDispatchRecorder!

    override func setUp() async throws {
        try await super.setUp()
        let made = await AppStateTestSupport.makeIsolatedStore()
        storeFileURL = made.fileURL
        store = made.store
        signer = RecordingSigner()
        kernelDispatches = KernelDispatchRecorder()
        // The wiring under test publishes through `store.identity` (the
        // AppStateStore-owned instance). Seed a `.localKey` signer so
        // `kernelSigningEnabled` is true, and install the kernel recorder so
        // the kernel dispatches are captured (no live kernel under XCTest).
        identity = store.identity
        identity._setSignerForTesting(signer)
        let recorder = kernelDispatches!
        identity._setKernelRecorderForTesting { namespace, body in
            recorder.record(namespace: namespace, body: body)
        }
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
        kernelDispatches = nil
        try await super.tearDown()
    }

    // MARK: - Step 1: identity → kernel wiring

    func testAdoptingLocalKeyDispatchesImportNsecToKernel() async throws {
        // Adopt a real local key (no Keychain) — `adoptLocal` runs the kernel
        // sync, which must forward the private key as ImportNsec.
        let pair = try NostrKeyPair.generate()
        identity._setLocalKeyForTesting(pair)

        let call = try XCTUnwrap(
            kernelDispatches.identity(type: "ImportNsec"),
            "Adopting a local key must dispatch podcast.identity ImportNsec."
        )
        XCTAssertEqual(call["nsec"] as? String, pair.privateKeyHex,
                       "ImportNsec must carry the local private key hex.")
    }

    func testClearIdentityDispatchesClearToKernel() async throws {
        // Sign-out MUST wipe the key from the kernel — otherwise it outlives
        // sign-out in the kernel IdentityStore and can still sign.
        identity.clearIdentity()
        XCTAssertNotNil(
            kernelDispatches.identity(type: "Clear"),
            "Sign-out must dispatch podcast.identity Clear."
        )
    }

    // MARK: - §5.3 row: Profile (kind:0, signs → kernel)

    func testPublishProfileDispatchesKindZeroToKernel() async throws {
        _ = try? await identity.publishProfile(
            name: "alice-test",
            displayName: "Alice",
            about: "Hello",
            picture: "https://example.test/a.png"
        )

        let call = try XCTUnwrap(
            kernelDispatches.social(op: "publish_profile"),
            "Expected one podcast.social publish_profile dispatch."
        )
        XCTAssertEqual(call["name"] as? String, "alice-test")
        XCTAssertEqual(call["display_name"] as? String, "Alice")
        XCTAssertEqual(call["about"] as? String, "Hello")
        XCTAssertEqual(call["picture"] as? String, "https://example.test/a.png")
        XCTAssertTrue(signer.calls.isEmpty, "Local-key profile must NOT sign Swift-side.")
    }

    // MARK: - §5.3 row: Notes (user) — kind 1 → kernel

    func testAddNoteUserAuthorDispatchesKindOneToKernel() async throws {
        _ = store.addNote(text: "first user note", kind: .free)
        try await waitForKernelDispatch(op: "publish_note")

        let call = try XCTUnwrap(kernelDispatches.social(op: "publish_note"))
        XCTAssertEqual(call["content"] as? String, "first user note")
        let tags = call["tags"] as? [[String]]
        XCTAssertTrue(tags?.contains(["t", "note"]) ?? false, "User notes must carry [\"t\", \"note\"] tag.")
        XCTAssertTrue(signer.calls.isEmpty, "Local-key notes must NOT sign Swift-side.")
    }

    func testAddNoteExplicitUserAuthorDispatchesKindOneToKernel() async throws {
        _ = store.addNote(text: "explicit user note", kind: .free, target: nil, author: .user)
        try await waitForKernelDispatch(op: "publish_note")

        let call = try XCTUnwrap(kernelDispatches.social(op: "publish_note"))
        XCTAssertEqual(call["content"] as? String, "explicit user note")
    }

    // MARK: - §5.3 row: Notes (agent tool) — does NOT publish

    func testAddNoteAgentAuthorDoesNotDispatch() async throws {
        _ = store.addNote(text: "agent note", kind: .free, target: nil, author: .agent)
        try await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertNil(kernelDispatches.social(op: "publish_note"),
                     "Agent-authored notes must not reach the kernel social path.")
        XCTAssertTrue(signer.calls.isEmpty, "Agent-authored notes must not sign Swift-side either.")
    }

    func testAgentToolCreateNoteDoesNotDispatch() async throws {
        _ = AgentTools.dispatchNotesMemory(
            name: AgentTools.Names.createNote,
            args: ["text": "agent tool note", "kind": "free"],
            store: store,
            batchID: UUID()
        )
        try await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertNil(kernelDispatches.social(op: "publish_note"),
                     "AgentTools.createNote must not reach the kernel social path.")
        XCTAssertEqual(store.state.notes.last?.text, "agent tool note")
        XCTAssertEqual(store.state.notes.last?.author, .agent)
    }

    // MARK: - §5.3 row: Memories — does NOT publish

    func testAddAgentMemoryDoesNotDispatch() async throws {
        _ = store.addAgentMemory(content: "long-running fact")
        try await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertTrue(kernelDispatches.socialCalls.isEmpty, "Memories must not reach the kernel social path.")
    }

    // MARK: - §5.3 row: Clips, source ≠ .agent — kind 9802 → kernel

    func testAddClipTouchSourceDispatchesKindNineEightZeroTwoToKernel() async throws {
        let clip = Clip(
            episodeID: UUID(),
            subscriptionID: UUID(),
            startMs: 1_000,
            endMs: 5_000,
            caption: "Worth re-listening",
            transcriptText: "the prose at the heart of the clip",
            source: .touch
        )
        store.addClip(clip)
        try await waitForKernelDispatch(op: "publish_highlight")

        let call = try XCTUnwrap(kernelDispatches.social(op: "publish_highlight"))
        XCTAssertEqual(call["content"] as? String, "the prose at the heart of the clip")
        let tags = call["tags"] as? [[String]]
        XCTAssertTrue(tags?.contains(["context", "the prose at the heart of the clip"]) ?? false,
                      "Clip must carry the [\"context\", transcript] tag.")
        XCTAssertTrue(tags?.contains(["alt", "Worth re-listening"]) ?? false,
                      "Clip with caption must carry the [\"alt\", caption] tag.")
        XCTAssertTrue(signer.calls.isEmpty, "Local-key clips must NOT sign Swift-side.")
    }

    func testAddClipAutoSourceDispatchesKindNineEightZeroTwoToKernel() async throws {
        let clip = Clip(
            episodeID: UUID(),
            subscriptionID: UUID(),
            startMs: 0,
            endMs: 1_000,
            transcriptText: "auto-snip text",
            source: .auto
        )
        store.addClip(clip)
        try await waitForKernelDispatch(op: "publish_highlight")
        XCTAssertNotNil(kernelDispatches.social(op: "publish_highlight"))
    }

    func testAddClipConvenienceOverloadDispatchesForNonAgentSource() async throws {
        _ = store.addClip(
            episodeID: UUID(),
            subscriptionID: UUID(),
            startMs: 0,
            endMs: 2_000,
            transcriptText: "auto-snip via convenience",
            source: .headphone
        )
        try await waitForKernelDispatch(op: "publish_highlight")
        XCTAssertNotNil(kernelDispatches.social(op: "publish_highlight"))
    }

    // MARK: - §5.3 row: Clips, source == .agent — does NOT publish

    func testAddClipAgentSourceDoesNotDispatch() async throws {
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
        XCTAssertNil(kernelDispatches.social(op: "publish_highlight"),
                     "Agent-sourced clips must not reach the kernel social path.")
        XCTAssertTrue(signer.calls.isEmpty, "Agent-sourced clips must not sign Swift-side either.")
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

    /// Polls until the kernel recorder has captured a `podcast.social`
    /// dispatch with the given `op`, or fails after a generous timeout.
    /// Wiring-layer publishes are fire-and-forget Tasks — there's no
    /// completion handle to await — so a short polling loop is the seam.
    private func waitForKernelDispatch(
        op: String,
        timeout: TimeInterval = 2.0,
        file: StaticString = #file,
        line: UInt = #line
    ) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while kernelDispatches.social(op: op) == nil {
            if Date() > deadline {
                XCTFail(
                    "Timed out waiting for podcast.social \(op) dispatch.",
                    file: file, line: line
                )
                return
            }
            try await Task.sleep(nanoseconds: 25_000_000)
        }
    }
}

// MARK: - KernelDispatchRecorder

/// Captures the `(namespace, body)` of every dispatch routed through
/// `UserIdentityStore.dispatchToKernel` so the wiring tests can assert
/// which publishes reached the kernel social path. Thread-safe — publish
/// Tasks fire off the main actor's run loop.
final class KernelDispatchRecorder: @unchecked Sendable {
    struct Call {
        let namespace: String
        let body: [String: Any]
    }

    private let queue = DispatchQueue(label: "KernelDispatchRecorder")
    private var _calls: [Call] = []

    func record(namespace: String, body: [String: Any]) {
        queue.sync { _calls.append(Call(namespace: namespace, body: body)) }
    }

    /// All `podcast.social` dispatch bodies, in order.
    var socialCalls: [[String: Any]] {
        queue.sync { _calls.filter { $0.namespace == "podcast.social" }.map(\.body) }
    }

    /// The first `podcast.social` dispatch body whose `op` matches, if any.
    func social(op: String) -> [String: Any]? {
        socialCalls.first { ($0["op"] as? String) == op }
    }

    /// The first `podcast.identity` dispatch body whose `type` matches, if any.
    func identity(type: String) -> [String: Any]? {
        let bodies = queue.sync {
            _calls.filter { $0.namespace == "podcast.identity" }.map(\.body)
        }
        return bodies.first { ($0["type"] as? String) == type }
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
