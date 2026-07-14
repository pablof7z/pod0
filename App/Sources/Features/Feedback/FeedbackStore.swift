import Foundation
import os
import SwiftUI

// MARK: - FeedbackStore

@MainActor
@Observable
final class FeedbackStore {

    private let logger = Logger.app("FeedbackStore")
    private let client = FeedbackRelayClient()

    var threads: [FeedbackThread] = []
    var isLoading: Bool = false
    var loadError: String?

    func load(identity: UserIdentityStore) async {
        isLoading = true
        loadError = nil
        do {
            let events = try await client.fetchProjectEvents(signer: identity.signer)
            threads = buildThreads(from: events, localPubkey: identity.publicKeyHex)
        } catch {
            logger.error("Failed to load feedback threads: \(error, privacy: .public)")
            loadError = error.localizedDescription
        }
        isLoading = false
    }

    @discardableResult
    func publishThread(
        category: FeedbackCategory,
        content: String,
        image: UIImage?,
        identity: UserIdentityStore
    ) async throws -> FeedbackThread {
        let event = try await identity.publishFeedbackNote(
            category: category,
            body: content,
            parentEventID: nil,
            replyToPubkey: nil
        )
        let thread = FeedbackThread(
            event: event,
            attachedImage: image,
            localPubkey: identity.publicKeyHex
        )
        threads.insert(thread, at: 0)
        return thread
    }

    func publishReply(content: String, threadID: String, identity: UserIdentityStore) async throws {
        guard let idx = threads.firstIndex(where: { $0.id == threadID }) else { return }
        let thread = threads[idx]
        let lastSpeaker = thread.replies.last?.authorPubkey ?? thread.authorPubkey
        let replyToPubkey = lastSpeaker == identity.publicKeyHex ? nil : lastSpeaker
        let event = try await identity.publishFeedbackNote(
            category: thread.category,
            body: content,
            parentEventID: thread.eventID,
            replyToPubkey: replyToPubkey
        )
        threads[idx].replies.append(FeedbackReply(event: event, localPubkey: identity.publicKeyHex))
    }

    func loadReplies(for thread: FeedbackThread, identity: UserIdentityStore) async {
        do {
            let events = try await client.fetchReplies(rootEventID: thread.eventID, signer: identity.signer)
            guard let idx = threads.firstIndex(where: { $0.id == thread.id }) else { return }
            let existing = Set(threads[idx].replies.map(\.eventID))
            let replies = events
                .filter { $0.kind == 1 && $0.rootEventID == thread.eventID && !existing.contains($0.id) }
                .sorted { $0.created_at < $1.created_at }
                .map { FeedbackReply(event: $0, localPubkey: identity.publicKeyHex) }
            threads[idx].replies.append(contentsOf: replies)
            applyLatestMetadata(from: events, to: idx)
        } catch {
            logger.error("Failed to load feedback replies: \(error, privacy: .public)")
        }
    }

    func deleteThread(id: String) {
        threads.removeAll { $0.id == id }
    }

    private func buildThreads(from events: [SignedNostrEvent], localPubkey: String?) -> [FeedbackThread] {
        let metas = events.filter { $0.kind == FeedbackRelayClient.metadataKind }
        let messages = events.filter { $0.kind == FeedbackRelayClient.textNoteKind }

        var metaByRoot: [String: FeedbackMetadata] = [:]
        for event in metas {
            guard let rootID = event.rootEventID else { continue }
            let parsed = FeedbackMetadata(event: event)
            if let existing = metaByRoot[rootID], existing.createdAt >= parsed.createdAt { continue }
            metaByRoot[rootID] = parsed
        }

        let replies = Dictionary(grouping: messages.filter { $0.rootEventID != nil }) {
            $0.rootEventID ?? ""
        }

        return messages
            .filter { $0.rootEventID == nil && $0.projectATags.contains(FeedbackRelayClient.projectCoordinate) }
            .sorted { $0.created_at > $1.created_at }
            .map { root in
                FeedbackThread(
                    event: root,
                    replies: (replies[root.id] ?? []).sorted { $0.created_at < $1.created_at },
                    metadata: metaByRoot[root.id],
                    localPubkey: localPubkey
                )
            }
    }

    private func applyLatestMetadata(from events: [SignedNostrEvent], to index: Int) {
        let rootID = threads[index].eventID
        guard let newest = events
            .filter({ $0.kind == FeedbackRelayClient.metadataKind && $0.rootEventID == rootID })
            .max(by: { $0.created_at < $1.created_at })
        else { return }
        let parsed = FeedbackMetadata(event: newest)
        threads[index].title = parsed.title ?? threads[index].title
        threads[index].summary = parsed.summary ?? threads[index].summary
        threads[index].statusLabel = parsed.statusLabel ?? threads[index].statusLabel
    }
}

// MARK: - FeedbackRelayClient

actor FeedbackRelayClient {
    static let textNoteKind = 1
    static let metadataKind = 513
    static let feedbackRelayURL = URL(string: "wss://relay.tenex.chat")!
    static let profileRelayURLs = [
        URL(string: "wss://relay.tenex.chat")!,
        URL(string: "wss://purplepag.es")!,
    ]
    static let projectCoordinate =
        "31933:09d48a1a5dbe13404a729634f1d6ba722d40513468dd713c8ea38ca9b7b6f2c7:podcast"

    private let relayURL: URL

    init(relayURL: URL = FeedbackRelayClient.feedbackRelayURL) {
        self.relayURL = relayURL
    }

    func fetchProjectEvents(signer: (any NostrSigner)? = nil) async throws -> [SignedNostrEvent] {
        try await fetch(filter: [
            "kinds": [Self.textNoteKind, Self.metadataKind],
            "#a": [Self.projectCoordinate],
        ], signer: signer)
    }

    func fetchReplies(rootEventID: String, signer: (any NostrSigner)? = nil) async throws -> [SignedNostrEvent] {
        try await fetch(filter: [
            "kinds": [Self.textNoteKind, Self.metadataKind],
            "#e": [rootEventID],
        ], signer: signer)
    }

    func fetchKind0(pubkeyHex: String) async throws -> [SignedNostrEvent] {
        try await fetch(filter: [
            "kinds": [0],
            "authors": [pubkeyHex],
            "limit": 1,
        ])
    }

    func publish(_ event: SignedNostrEvent, authSigner: (any NostrSigner)?) async throws {
        let task = URLSession.shared.webSocketTask(with: relayURL)
        task.resume()
        defer { task.cancel(with: .normalClosure, reason: nil) }

        try await sendEvent(event, task: task)

        var authEventID: String?
        var retriedAfterAuth = false
        while let text = try await receiveText(task: task, timeoutSeconds: 8) {
            guard let message = Self.parseMessage(text) else { continue }
            switch message {
            case .ok(let eventID, let accepted, let reason):
                if eventID == event.id {
                    if accepted { return }
                    if reason?.hasPrefix("auth-required:") == true, authSigner != nil { continue }
                    throw FeedbackRelayError.rejected(reason ?? "Relay rejected feedback.")
                }
                if eventID == authEventID, accepted, !retriedAfterAuth {
                    retriedAfterAuth = true
                    try await sendEvent(event, task: task)
                } else if eventID == authEventID, !accepted {
                    throw FeedbackRelayError.rejected(reason ?? "Relay rejected authentication.")
                }
            case .auth(let challenge):
                guard let authSigner else { throw FeedbackRelayError.authRequired }
                let auth = try await authSigner.sign(NostrEventDraft(
                    kind: 22242,
                    content: "",
                    tags: [["relay", relayURL.absoluteString], ["challenge", challenge]]
                ))
                authEventID = auth.id
                try await send(["AUTH", eventDictionary(auth)], task: task)
            case .notice(let message):
                throw FeedbackRelayError.rejected(message)
            case .event, .eose, .closed:
                continue
            }
        }
        throw FeedbackRelayError.timeout
    }

    private func fetch(filter: [String: Any], signer: (any NostrSigner)? = nil) async throws -> [SignedNostrEvent] {
        let task = URLSession.shared.webSocketTask(with: relayURL)
        task.resume()
        defer { task.cancel(with: .normalClosure, reason: nil) }

        let subscriptionID = "feedback-\(UUID().uuidString)"
        try await send(["REQ", subscriptionID, filter], task: task)

        var events: [SignedNostrEvent] = []
        var authenticated = false
        var reqResentAfterAuth = false
        while let text = try await receiveText(task: task, timeoutSeconds: 8) {
            guard let message = Self.parseMessage(text) else { continue }
            switch message {
            case .event(let subID, let event):
                guard subID == subscriptionID else { continue }
                events.append(event)
            case .eose(let subID):
                guard subID == subscriptionID else { continue }
                return events
            case .closed(let subID, let reason):
                guard subID == subscriptionID else { continue }
                if reason?.hasPrefix("auth-required:") == true, authenticated, !reqResentAfterAuth {
                    reqResentAfterAuth = true
                    try await send(["REQ", subscriptionID, filter], task: task)
                } else {
                    return events
                }
            case .auth(let challenge):
                guard let signer else { continue }
                let authEvent = try await signer.sign(NostrEventDraft(
                    kind: 22242,
                    content: "",
                    tags: [["relay", relayURL.absoluteString], ["challenge", challenge]]
                ))
                try await send(["AUTH", eventDictionary(authEvent)], task: task)
                authenticated = true
            case .notice(let message):
                throw FeedbackRelayError.rejected(message)
            case .ok:
                continue
            }
        }
        return events
    }

    private func sendEvent(_ event: SignedNostrEvent, task: URLSessionWebSocketTask) async throws {
        try await send(["EVENT", eventDictionary(event)], task: task)
    }

    private func send(_ message: [Any], task: URLSessionWebSocketTask) async throws {
        let data = try JSONSerialization.data(withJSONObject: message)
        guard let text = String(data: data, encoding: .utf8) else {
            throw FeedbackRelayError.encodingFailed
        }
        try await task.send(.string(text))
    }

    private func receiveText(task: URLSessionWebSocketTask, timeoutSeconds: Double) async throws -> String? {
        try await withThrowingTaskGroup(of: String?.self) { group in
            group.addTask {
                let message = try await task.receive()
                if case .string(let text) = message { return text }
                return nil
            }
            group.addTask {
                try await Task.sleep(nanoseconds: UInt64(timeoutSeconds * 1_000_000_000))
                return nil
            }
            let result = try await group.next() ?? nil
            group.cancelAll()
            return result
        }
    }

    private func eventDictionary(_ event: SignedNostrEvent) -> [String: Any] {
        [
            "id": event.id,
            "pubkey": event.pubkey,
            "created_at": event.created_at,
            "kind": event.kind,
            "tags": event.tags,
            "content": event.content,
            "sig": event.sig,
        ]
    }

    private static func parseMessage(_ text: String) -> RelayMessage? {
        guard let data = text.data(using: .utf8),
              let array = try? JSONSerialization.jsonObject(with: data) as? [Any],
              let head = array.first as? String else { return nil }
        switch head {
        case "EVENT":
            guard array.count >= 3,
                  let subID = array[1] as? String,
                  let event = decodeEvent(array[2]) else { return nil }
            return .event(subID, event)
        case "EOSE":
            guard array.count >= 2, let subID = array[1] as? String else { return nil }
            return .eose(subID)
        case "OK":
            guard array.count >= 3,
                  let eventID = array[1] as? String,
                  let accepted = array[2] as? Bool else { return nil }
            return .ok(eventID, accepted, array.count >= 4 ? array[3] as? String : nil)
        case "AUTH":
            guard array.count >= 2, let challenge = array[1] as? String else { return nil }
            return .auth(challenge)
        case "NOTICE":
            return .notice(array.count >= 2 ? (array[1] as? String ?? "Relay notice.") : "Relay notice.")
        case "CLOSED":
            guard array.count >= 2, let subID = array[1] as? String else { return nil }
            return .closed(subID, array.count >= 3 ? array[2] as? String : nil)
        default:
            return nil
        }
    }

    private static func decodeEvent(_ value: Any) -> SignedNostrEvent? {
        guard JSONSerialization.isValidJSONObject(value),
              let data = try? JSONSerialization.data(withJSONObject: value) else { return nil }
        return try? Self.eventDecoder.decode(SignedNostrEvent.self, from: data)
    }

    /// Shared. `decodeEvent` runs for every Nostr event the relay
    /// streams (text notes + metadata for the feedback project), so
    /// a busy session reallocated a `JSONDecoder` per event.
    nonisolated(unsafe) private static let eventDecoder = JSONDecoder()

    private enum RelayMessage {
        case event(String, SignedNostrEvent)
        case eose(String)
        case ok(String, Bool, String?)
        case auth(String)
        case notice(String)
        case closed(String, String?)
    }
}

enum FeedbackRelayError: LocalizedError {
    case authRequired
    case encodingFailed
    case rejected(String)
    case timeout

    var errorDescription: String? {
        switch self {
        case .authRequired:
            "Relay requires authentication, but no signer is available."
        case .encodingFailed:
            "Could not encode the feedback event."
        case .rejected(let message):
            message
        case .timeout:
            "Feedback relay did not respond in time."
        }
    }
}
