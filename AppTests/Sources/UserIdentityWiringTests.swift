import XCTest
@testable import Podcastr

// MARK: - UserIdentityWiringTests
//
// Table-driven coverage for the "what publishes vs. what stays local" matrix.
// Every user-authored artefact still owned by this Swift store must dispatch
// a `podcast.social.*` op to the kernel; every agent-authored artefact must
// NOT. The kernel owns ALL signing — there is no Swift signer
// anymore — so these tests assert against a recording KERNEL seam
// (`_setKernelRecorderForTesting`): the "publishes" rows assert a dispatch
// reached it, the "does not publish" rows assert none did. The publish/relay
// leg is owned by the kernel and not exercised here.

@MainActor
final class UserIdentityWiringTests: XCTestCase {

    private var storeFileURL: URL!
    private var store: AppStateStore!
    private var identity: UserIdentityStore!
    private var kernelDispatches: KernelDispatchRecorder!

    override func setUp() async throws {
        try await super.setUp()
        let made = await AppStateTestSupport.makeIsolatedStore()
        storeFileURL = made.fileURL
        store = made.store
        kernelDispatches = KernelDispatchRecorder()
        // The wiring under test publishes through `store.identity` (the
        // AppStateStore-owned instance). Seed an active `.localKey` account so
        // readiness checks pass, and install the kernel recorder so the kernel
        // dispatches are captured (no live kernel under XCTest).
        identity = store.identity
        identity._setActiveAccountForTesting(String(repeating: "0", count: 64))
        let recorder = kernelDispatches!
        identity._setKernelRecorderForTesting { namespace, body in
            recorder.record(namespace: namespace, body: body)
        }
    }

    override func tearDown() async throws {
        identity._clearActiveAccountForTesting()
        if let storeFileURL {
            AppStateTestSupport.disposeIsolatedStore(at: storeFileURL)
        }
        store = nil
        storeFileURL = nil
        identity = nil
        kernelDispatches = nil
        try await super.tearDown()
    }

    // MARK: - Sign-out → kernel Clear

    func testClearIdentityDispatchesClearToKernel() async throws {
        // Sign-out MUST wipe the key from the kernel — otherwise it outlives
        // sign-out in the kernel identity store and can still sign.
        identity.clearIdentity()
        XCTAssertNotNil(
            kernelDispatches.identity(type: "Clear"),
            "Sign-out must dispatch podcast.identity Clear."
        )
    }

    // MARK: - Profile (kind:0, publishes → kernel)

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
    }

    // MARK: - Notes (user) — kind 1 → kernel

    func testAddNoteUserAuthorDispatchesKindOneToKernel() async throws {
        _ = store.addNote(text: "first user note", kind: .free)
        try await waitForKernelDispatch(op: "publish_note")

        let call = try XCTUnwrap(kernelDispatches.social(op: "publish_note"))
        XCTAssertEqual(call["content"] as? String, "first user note")
        // Tag construction (the `["t","note"]` marker) moved into the Rust
        // kernel in #355 — Swift dispatches typed fields, the kernel builds
        // the NIP tags (covered by `build_note_tags` unit tests in
        // `apps/nmp-app-podcast/src/social_publish_handler_tests.rs`). The
        // Swift wiring's contract is therefore: dispatch the content and do
        // NOT pre-build tags. No episode coord is supplied at this call site,
        // so no `episode_coord` field is dispatched either.
        XCTAssertNil(call["tags"], "Swift must not pre-build tags; the kernel owns NIP tag construction (#355).")
        XCTAssertNil(call["episode_coord"], "No episode coord at this call site, so none should be dispatched.")
    }

    func testAddNoteExplicitUserAuthorDispatchesKindOneToKernel() async throws {
        _ = store.addNote(text: "explicit user note", kind: .free, target: nil, author: .user)
        try await waitForKernelDispatch(op: "publish_note")

        let call = try XCTUnwrap(kernelDispatches.social(op: "publish_note"))
        XCTAssertEqual(call["content"] as? String, "explicit user note")
    }

    // MARK: - Notes (agent tool) — does NOT publish

    func testAddNoteAgentAuthorDoesNotDispatch() async throws {
        _ = store.addNote(text: "agent note", kind: .free, target: nil, author: .agent)
        try await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertNil(kernelDispatches.social(op: "publish_note"),
                     "Agent-authored notes must not reach the kernel social path.")
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
