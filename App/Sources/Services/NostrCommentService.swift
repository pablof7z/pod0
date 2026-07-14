import Foundation
import os.log

// MARK: - NostrCommentService
//
// Standalone service for NIP-22 (kind 1111) comments anchored to NIP-73
// external content identifiers. Each `subscribe(target:)` call opens its
// own WebSocket session to the user's configured relay — the existing
// `NostrRelayService` is kept single-purpose (friend-DM inbox) so the two
// concerns can evolve independently. Multiple in-flight subscriptions are
// supported: each one has a unique REQ id and gets its own backing
// `AsyncStream`.
//
// Publish path goes through whatever `NostrSigner` the caller hands in —
// `UserIdentityStore.signer` resolves to a `LocalKeySigner` or a
// `RemoteSigner` (NIP-46), so this service stays agnostic to where the
// user's key lives.
//
// NIP-22 wire shape (top-level comment on external content):
//   kind: 1111
//   content: "<plain text>"
//   tags: [
//     ["I", "<root nip73 identifier>"],
//     ["K", "<root nip73 kind>"],
//     ["i", "<parent nip73 identifier>"],   // == "I" for top-level
//     ["k", "<parent nip73 kind>"],         // == "K" for top-level
//   ]
//
// We omit reply tags (uppercase E/K of the parent comment) — phase 1
// supports top-level comments only.
@MainActor
final class NostrCommentService {

    nonisolated private static let logger = Logger.app("NostrCommentService")

    // MARK: - Wire constants

    private enum Wire {
        static let kindComment = 1111
        static let req = "REQ"
        static let event = "EVENT"
        static let close = "CLOSE"
        static let reconnectDelay: Duration = .seconds(5)
    }

    // MARK: - Subscription handle

    /// Returned to the caller so they can cancel the subscription when the
    /// view disappears. Holding the handle keeps the websocket open.
    final class Subscription {
        let stream: AsyncStream<EpisodeComment>
        private let cancelClosure: @Sendable () -> Void
        init(stream: AsyncStream<EpisodeComment>, cancel: @escaping @Sendable () -> Void) {
            self.stream = stream
            self.cancelClosure = cancel
        }
        func cancel() { cancelClosure() }
        deinit { cancelClosure() }
    }

    // MARK: - Per-subscription session state

    private final class Session {
        let id: String
        let target: CommentTarget
        let continuation: AsyncStream<EpisodeComment>.Continuation
        var webSocket: URLSessionWebSocketTask?
        var receiveLoop: Task<Void, Never>?
        /// Dedup ring so a relay returning the same event twice (via reconnect
        /// + filter replay) doesn't double-render in the UI.
        var seenIDs: Set<String> = []

        init(id: String, target: CommentTarget, continuation: AsyncStream<EpisodeComment>.Continuation) {
            self.id = id
            self.target = target
            self.continuation = continuation
        }
    }

    // MARK: - Deps

    private let relayURLProvider: @MainActor () -> URL?
    /// Active sessions keyed by REQ id. The `Subscription` returned to the
    /// caller carries that id and a weak service ref, so cancellation can
    /// route here without capturing the non-Sendable `Session` directly in
    /// a `@Sendable` closure.
    private var sessions: [String: Session] = [:]

    init(relayURLProvider: @MainActor @escaping () -> URL?) {
        self.relayURLProvider = relayURLProvider
    }

    /// Convenience initializer that reads the relay URL from
    /// `AppStateStore.state.settings.nostrRelayURL`. Use this everywhere
    /// except tests; injection above stays for unit-test wiring.
    convenience init(store: AppStateStore) {
        self.init(relayURLProvider: {
            let raw = store.state.settings.nostrRelayURL
            return raw.isEmpty ? nil : URL(string: raw)
        })
    }

    // MARK: - Subscribe

    /// Opens a websocket, sends a REQ filtered to `target`, yields each
    /// matching comment into the returned stream. Reconnects on transient
    /// websocket failure (every 5s) until the caller cancels.
    func subscribe(target: CommentTarget) -> Subscription {
        let (stream, continuation) = AsyncStream<EpisodeComment>.makeStream(bufferingPolicy: .unbounded)
        let id = "cmt-\(UUID().uuidString.prefix(8))"
        let session = Session(id: id, target: target, continuation: continuation)
        sessions[id] = session
        connect(session: session)

        // The cancel closure captures only Sendable values (`id`, weak self).
        // Tearing down a non-Sendable `Session` directly from `@Sendable`
        // context is rejected by Swift 6 strict concurrency.
        return Subscription(stream: stream, cancel: { [weak self] in
            Task { @MainActor in
                self?.tearDownSession(id: id)
            }
        })
    }

    private func tearDownSession(id: String) {
        guard let session = sessions.removeValue(forKey: id) else { return }
        session.receiveLoop?.cancel()
        session.webSocket?.cancel(with: .goingAway, reason: nil)
        session.continuation.finish()
    }

    private func connect(session: Session) {
        guard let url = relayURLProvider() else {
            Self.logger.info("NostrCommentService: no relay configured — skipping subscribe for \(session.id, privacy: .public)")
            session.continuation.finish()
            return
        }
        let task = URLSession.shared.webSocketTask(with: url)
        session.webSocket = task
        task.resume()
        sendREQ(session: session)
        startReceive(session: session)
    }

    private func sendREQ(session: Session) {
        let filter: [String: Any] = [
            "kinds": [Wire.kindComment],
            "#i": [session.target.nip73Identifier],
            "limit": 200,
        ]
        let message: [Any] = [Wire.req, session.id, filter]
        sendJSON(message, on: session.webSocket, label: "REQ \(session.id)")
    }

    private func startReceive(session: Session) {
        session.receiveLoop = Task { @MainActor [weak self, weak session] in
            guard let self, let session else { return }
            while !Task.isCancelled, let task = session.webSocket {
                do {
                    let msg = try await task.receive()
                    if case .string(let text) = msg {
                        self.ingest(text: text, into: session)
                    }
                } catch {
                    if Task.isCancelled { return }
                    Self.logger.warning("NostrCommentService: socket error on \(session.id, privacy: .public) — \(error, privacy: .public); reconnecting")
                    try? await Task.sleep(for: Wire.reconnectDelay)
                    if Task.isCancelled { return }
                    self.connect(session: session)
                    return
                }
            }
        }
    }

    private func ingest(text: String, into session: Session) {
        guard let data = text.data(using: .utf8),
              let array = try? JSONSerialization.jsonObject(with: data) as? [Any],
              array.count >= 3,
              let head = array[0] as? String, head == Wire.event,
              let event = array[2] as? [String: Any],
              let kind = event["kind"] as? Int, kind == Wire.kindComment,
              let id = event["id"] as? String,
              let pubkey = event["pubkey"] as? String,
              let createdAt = event["created_at"] as? Int,
              let content = event["content"] as? String else { return }

        guard !session.seenIDs.contains(id) else { return }
        session.seenIDs.insert(id)

        let comment = EpisodeComment(
            id: id,
            target: session.target,
            authorPubkeyHex: pubkey,
            content: content,
            createdAt: Date(timeIntervalSince1970: TimeInterval(createdAt))
        )
        session.continuation.yield(comment)
    }

    // MARK: - Publish

    /// Sign a NIP-22 comment for `target` with the supplied signer, then
    /// publish it to the user's relay. Returns the signed event so the UI
    /// can optimistically append before the relay echoes it back through a
    /// live subscription.
    func publish(
        content: String,
        target: CommentTarget,
        signer: any NostrSigner
    ) async throws -> SignedNostrEvent {
        let trimmed = content.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { throw PublishError.emptyContent }
        guard let url = relayURLProvider() else { throw PublishError.noRelayConfigured }

        let identifier = target.nip73Identifier
        let kindTag = target.nip73Kind
        // Top-level comment: parent (`i`/`k`) == root (`I`/`K`).
        let tags: [[String]] = [
            ["I", identifier],
            ["K", kindTag],
            ["i", identifier],
            ["k", kindTag],
        ]
        let draft = NostrEventDraft(kind: Wire.kindComment, content: trimmed, tags: tags)
        let event = try await signer.sign(draft)

        try await publishSignedEvent(event, to: url)
        return event
    }

    private func publishSignedEvent(_ event: SignedNostrEvent, to url: URL) async throws {
        let task = URLSession.shared.webSocketTask(with: url)
        task.resume()
        defer { task.cancel(with: .normalClosure, reason: nil) }

        let eventDict: [String: Any] = [
            "id": event.id,
            "pubkey": event.pubkey,
            "created_at": event.created_at,
            "kind": event.kind,
            "tags": event.tags,
            "content": event.content,
            "sig": event.sig,
        ]
        let message: [Any] = [Wire.event, eventDict]
        let data = try JSONSerialization.data(withJSONObject: message, options: [])
        guard let text = String(data: data, encoding: .utf8) else {
            throw PublishError.encodingFailed
        }
        try await task.send(.string(text))
        // Best-effort: wait for an OK frame from the relay, but don't block
        // the UI forever if the relay is slow. Most public relays reply in
        // <500ms with `["OK", <id>, true, ""]`.
        do {
            let response = try await withThrowingTaskGroup(of: URLSessionWebSocketTask.Message.self) { group in
                group.addTask { try await task.receive() }
                group.addTask {
                    try? await Task.sleep(for: .seconds(3))
                    throw PublishError.relayAckTimeout
                }
                let result = try await group.next()!
                group.cancelAll()
                return result
            }
            if case .string(let text) = response,
               let data = text.data(using: .utf8),
               let array = try? JSONSerialization.jsonObject(with: data) as? [Any],
               array.count >= 3,
               let head = array[0] as? String, head == "OK",
               let accepted = array[2] as? Bool, !accepted {
                let reason = (array.count > 3 ? (array[3] as? String) : nil) ?? "relay rejected event"
                throw PublishError.relayRejected(reason)
            }
        } catch PublishError.relayAckTimeout {
            // Soft timeout — the event probably landed, but we can't
            // confirm. UI optimistically shows the comment either way.
            Self.logger.notice("publish: no ack from relay within 3s; assuming success")
        }
    }

    // MARK: - JSON send helper

    private func sendJSON(_ message: [Any], on task: URLSessionWebSocketTask?, label: String) {
        guard let task else { return }
        do {
            let data = try JSONSerialization.data(withJSONObject: message)
            guard let text = String(data: data, encoding: .utf8) else { return }
            task.send(.string(text)) { error in
                if let error {
                    Self.logger.error("\(label, privacy: .public): send failed — \(error, privacy: .public)")
                }
            }
        } catch {
            Self.logger.error("\(label, privacy: .public): serialization failed — \(error, privacy: .public)")
        }
    }

    // MARK: - Errors

    enum PublishError: LocalizedError {
        case emptyContent
        case noRelayConfigured
        case encodingFailed
        case relayRejected(String)
        case relayAckTimeout

        var errorDescription: String? {
            switch self {
            case .emptyContent:        "Comment is empty."
            case .noRelayConfigured:   "Set a Nostr relay URL in Settings before commenting."
            case .encodingFailed:      "Couldn't encode the comment for the relay."
            case .relayRejected(let m): "Relay rejected the comment: \(m)"
            case .relayAckTimeout:     "Relay didn't acknowledge in time."
            }
        }
    }
}
