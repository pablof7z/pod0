import Foundation
import os.log
import UIKit

/// Connects to a configured Nostr relay and gates incoming kind:1 messages
/// through the access-control layer, queuing unknown senders for user approval.
@MainActor
final class NostrRelayService {
    nonisolated private static let logger = Logger.app("NostrRelayService")
    private let store: AppStateStore
    /// Owner-consultation surface from AppMain. Injected at init so the
    /// responder has it before `RootView.onAppear` fires — closes the
    /// cold-launch race where an inbound landing in the first few
    /// seconds would otherwise see `askCoordinator == nil` and have the
    /// `ask` tool short-circuit to an error envelope.
    private weak var askCoordinator: AgentAskCoordinator?
    private var webSocketTask: URLSessionWebSocketTask?
    private var receiveLoop: Task<Void, Never>?
    private var connectedRelayURL: String?
    private lazy var profileFetcher = NostrProfileFetcher(store: store)
    /// Owns the inbound → LLM → outbound pipeline for allowed pubkeys.
    /// Kept lazy so apps with Nostr disabled never instantiate it.
    /// Exposed (read-only) so `RootView` can late-bind the podcast tool
    /// deps once the UI mounts. `askCoordinator` is plumbed here at
    /// init time and applied below.
    lazy var agentResponder: NostrAgentResponder = {
        let responder = NostrAgentResponder(store: store)
        responder.askCoordinator = askCoordinator
        return responder
    }()
    /// Tracks pubkeys we've already queued a profile fetch for during this
    /// session so a burst of inbound events from the same peer doesn't
    /// spam the relay with kind:0 requests. Cleared on `stop()`.
    private var profileFetchInflight: Set<String> = []
    /// NIP-42: ids of kind:22242 AUTH events we sent and are waiting on. When
    /// the relay returns an accepted OK for one of these, we re-issue the REQ
    /// since auth-gated relays drop pre-auth subscriptions.
    private var pendingAuthEventIDs: Set<String> = []
    /// Agent pubkey of the live connection; kept so we can re-send the REQ
    /// after NIP-42 AUTH is accepted without plumbing it back through.
    private var currentAgentPubkey: String?

    // MARK: - Protocol constants

    private enum NostrProtocol {
        static let requestCommand = "REQ"
        static let authMessage = "AUTH"
        static let eventMessage = "EVENT"
        static let kindTextNote = 1
        static let kindAuth = 22242
        static let minEventArrayCount = 3
        static let eventIndex = 2
        static let subscriptionID = "agent-inbox"
        static let reconnectDelay: Duration = .seconds(5)
    }

    /// Creates a new service backed by the given state store.
    /// `askCoordinator` is wired through to the lazy `agentResponder` on
    /// first access so peer-initiated `ask` tool calls can pop the
    /// owner-consent sheet from cold-launch onward.
    init(store: AppStateStore, askCoordinator: AgentAskCoordinator? = nil) {
        self.store = store
        self.askCoordinator = askCoordinator
    }

    // MARK: - Lifecycle

    /// Reads the current Nostr settings and connects to the configured relay,
    /// or stops the service if Nostr is disabled or misconfigured.
    func start() {
        let settings = store.state.settings
        guard settings.nostrEnabled,
              let pubkeyHex = settings.nostrPublicKeyHex, !pubkeyHex.isEmpty,
              !settings.nostrRelayURL.isEmpty else {
            NostrRelayService.logger.notice(
                "start: skipping — enabled=\(settings.nostrEnabled, privacy: .public), hasPubkey=\(settings.nostrPublicKeyHex?.isEmpty == false, privacy: .public), relayURL='\(settings.nostrRelayURL, privacy: .public)'"
            )
            stop()
            return
        }
        guard connectedRelayURL != settings.nostrRelayURL || webSocketTask == nil else {
            NostrRelayService.logger.debug("start: already connected to \(settings.nostrRelayURL, privacy: .public); no-op")
            return
        }
        NostrRelayService.logger.notice(
            "start: connecting agent \(pubkeyHex.prefix(12), privacy: .public)… on \(settings.nostrRelayURL, privacy: .public)"
        )
        stop()
        connect(urlString: settings.nostrRelayURL, agentPubkey: pubkeyHex)
    }

    /// Cancels the receive loop and closes the WebSocket connection.
    func stop() {
        receiveLoop?.cancel()
        receiveLoop = nil
        webSocketTask?.cancel(with: .goingAway, reason: nil)
        webSocketTask = nil
        connectedRelayURL = nil
        currentAgentPubkey = nil
        pendingAuthEventIDs.removeAll()
        profileFetchInflight.removeAll()
    }

    // MARK: - Connection

    private func connect(urlString: String, agentPubkey: String) {
        guard let url = URL(string: urlString) else {
            NostrRelayService.logger.error("NostrRelayService: invalid relay URL '\(urlString, privacy: .public)'")
            return
        }
        connectedRelayURL = urlString
        currentAgentPubkey = agentPubkey
        NostrRelayService.logger.info("NostrRelayService: connecting to \(urlString, privacy: .public)")
        let task = URLSession.shared.webSocketTask(with: url)
        webSocketTask = task
        task.resume()
        sendSubscription(agentPubkey: agentPubkey)
        startReceiveLoop(agentPubkey: agentPubkey)
        publishAgentProfileIfPossible(relayURL: url)
    }

    // MARK: - Kind:0 profile publish

    /// Publishes the agent's kind:0 metadata event to the configured relay.
    /// Can be called directly when the user edits profile settings without
    /// restarting the relay connection.
    func republishProfile() {
        let settings = store.state.settings
        guard settings.nostrEnabled,
              settings.nostrPublicKeyHex?.isEmpty == false,
              !settings.nostrRelayURL.isEmpty,
              let relayURL = URL(string: settings.nostrRelayURL) else { return }
        publishAgentProfileIfPossible(relayURL: relayURL)
    }

    private func publishAgentProfileIfPossible(relayURL: URL) {
        let settings = store.state.settings
        let name = settings.nostrProfileName.trimmed
        let about = settings.nostrProfileAbout.trimmed
        let picture = settings.nostrProfilePicture.trimmed
        let effectiveName = name.isEmpty ? "Podcastr Agent" : name
        let deviceName = UIDevice.current.name

        Task {
            guard let privKey = try? NostrCredentialStore.privateKey() else { return }
            guard let pair = try? NostrKeyPair(privateKeyHex: privKey) else { return }
            var metadata: [String: String] = ["name": effectiveName, "about": about]
            if !picture.isEmpty { metadata["picture"] = picture }
            guard let data = try? JSONSerialization.data(withJSONObject: metadata, options: [.sortedKeys]),
                  let content = String(data: data, encoding: .utf8) else { return }
            let draft = NostrEventDraft(kind: 0, content: content, tags: [["backend", "Podcastr App in \(deviceName)"]])
            guard let event = try? await LocalKeySigner(keyPair: pair).sign(draft) else { return }
            let publisher = NostrWebSocketEventPublisher()
            try? await publisher.publish(event: event, relayURL: relayURL)
        }
    }

    private func sendSubscription(agentPubkey: String) {
        // NIP-10 parity with win-the-day: carry `since:` from the
        // persisted cursor so a reconnecting agent doesn't have to chew
        // through every kind:1 the relay has ever seen tagged to it.
        // The dedup set (`nostrRespondedEventIDs`) protects against the
        // tiny overlap when an event with `created_at == cursor` is
        // re-delivered. Omit the field on cold first launch so the
        // initial inbox sync still pulls historic mentions.
        var filter: [String: Any] = [
            "kinds": [NostrProtocol.kindTextNote],
            "#p": [agentPubkey],
        ]
        if let since = store.state.nostrSinceCursor {
            filter["since"] = since
        }
        let message: [Any] = [NostrProtocol.requestCommand, NostrProtocol.subscriptionID, filter]
        do {
            let data = try JSONSerialization.data(withJSONObject: message)
            guard let text = String(data: data, encoding: .utf8) else {
                NostrRelayService.logger.error("sendSubscription: failed to encode REQ as UTF-8 string")
                return
            }
            NostrRelayService.logger.notice("sendSubscription: REQ \(text, privacy: .public)")
            webSocketTask?.send(.string(text)) { error in
                if let error {
                    NostrRelayService.logger.error("sendSubscription: WebSocket send failed — \(error, privacy: .public)")
                } else {
                    NostrRelayService.logger.notice("sendSubscription: REQ sent OK")
                }
            }
        } catch {
            NostrRelayService.logger.error("sendSubscription: JSON serialization failed — \(error, privacy: .public)")
        }
    }

    private func startReceiveLoop(agentPubkey: String) {
        receiveLoop = Task { @MainActor [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                guard let task = self.webSocketTask else { return }
                do {
                    let msg = try await task.receive()
                    if case .string(let text) = msg { self.handle(text: text) }
                } catch {
                    guard !Task.isCancelled else { return }
                    NostrRelayService.logger.warning("NostrRelayService: WebSocket error — \(error, privacy: .public); reconnecting in \(NostrProtocol.reconnectDelay)")
                    // Cancellation is checked immediately after; swallowing the sleep-cancel error is intentional.
                    try? await Task.sleep(for: NostrProtocol.reconnectDelay)
                    guard !Task.isCancelled else { return }
                    self.start()
                    return
                }
            }
        }
    }

    // MARK: - Event handling

    private func handle(text: String) {
        NostrRelayService.logger.debug("relay frame: \(text, privacy: .public)")
        guard let data = text.data(using: .utf8),
              let array = try? JSONSerialization.jsonObject(with: data) as? [Any],
              array.count >= 2,
              let msgType = array[0] as? String else {
            NostrRelayService.logger.notice("handle: dropped unparseable frame")
            return
        }

        // Surface relay-side acks/errors so we can tell the difference between
        // "no events delivered" and "the subscription was rejected".
        switch msgType {
        case "NOTICE":
            NostrRelayService.logger.notice("relay NOTICE: \(text, privacy: .public)")
            return
        case "CLOSED":
            NostrRelayService.logger.notice("relay CLOSED: \(text, privacy: .public)")
            return
        case "OK":
            NostrRelayService.logger.notice("relay OK: \(text, privacy: .public)")
            handleOK(array: array)
            return
        case "EOSE":
            NostrRelayService.logger.notice("relay EOSE: \(text, privacy: .public)")
            return
        case NostrProtocol.authMessage:
            handleAuthChallenge(array: array)
            return
        case NostrProtocol.eventMessage:
            break
        default:
            return
        }

        guard array.count >= NostrProtocol.minEventArrayCount,
              let event = array[NostrProtocol.eventIndex] as? [String: Any],
              let kind = event["kind"] as? Int,
              let senderPubkey = event["pubkey"] as? String,
              let eventID = event["id"] as? String else {
            NostrRelayService.logger.notice("handle: dropped EVENT with missing fields")
            return
        }

        NostrRelayService.logger.notice(
            "inbound kind=\(kind, privacy: .public) id=\(eventID.prefix(12), privacy: .public) from=\(senderPubkey.prefix(12), privacy: .public)"
        )

        guard kind == NostrProtocol.kindTextNote else { return }
        guard senderPubkey != store.state.settings.nostrPublicKeyHex else {
            NostrRelayService.logger.debug("handle: dropping self-authored event")
            return
        }
        guard !store.state.nostrBlockedPubkeys.contains(senderPubkey) else {
            NostrRelayService.logger.notice("handle: dropping event from blocked pubkey")
            return
        }

        // Delegation responses: a reply to one of our `send_friend_message`
        // root events. Route directly to the responder without requiring the
        // sender to be in the allowlist — the pending-message registry acts
        // as the authorization gate.
        let inboundTags = (event["tags"] as? [[String]]) ?? []
        let inboundRootID = NostrConversationRoot.rootEventID(
            eventID: eventID,
            tags: inboundTags
        )
        if store.hasPendingFriendMessage(forRootEventID: inboundRootID) {
            NostrRelayService.logger.notice(
                "handle: routing delegation response from \(senderPubkey.prefix(12), privacy: .public) to agent responder"
            )
            ensureProfileFetch(for: senderPubkey)
            agentResponder.handle(inbound: NostrAgentResponder.Inbound(
                eventID: eventID,
                pubkey: senderPubkey,
                createdAt: (event["created_at"] as? Int) ?? Int(Date().timeIntervalSince1970),
                content: (event["content"] as? String) ?? "",
                tags: inboundTags,
                rawEventJSON: rawEventJSON(from: event)
            ))
            return
        }

        if store.state.nostrAllowedPubkeys.contains(senderPubkey) {
            NostrRelayService.logger.notice("handle: routing inbound from allowed pubkey to agent responder")
            // Kick off a kind:0 fetch in parallel with the responder so
            // the conversations UI and approval views see the peer's
            // display name + avatar even when this inbound is a follow-
            // up turn the responder doesn't need to fetch profile for
            // again. The responder runs its own bounded 2s profile
            // race for cold-cache cases; the two fetches are independent
            // (different in-flight guards) and slightly wasteful in the
            // worst case — preferable to leaving the UI cache cold.
            ensureProfileFetch(for: senderPubkey)
            agentResponder.handle(inbound: NostrAgentResponder.Inbound(
                eventID: eventID,
                pubkey: senderPubkey,
                createdAt: (event["created_at"] as? Int) ?? Int(Date().timeIntervalSince1970),
                content: (event["content"] as? String) ?? "",
                tags: (event["tags"] as? [[String]]) ?? [],
                rawEventJSON: rawEventJSON(from: event)
            ))
            return
        }

        let isNew = !store.state.nostrPendingApprovals.contains { $0.pubkeyHex == senderPubkey }
        NostrRelayService.logger.notice("handle: queueing approval (new=\(isNew, privacy: .public)) for \(senderPubkey.prefix(12), privacy: .public)")
        let cached = store.state.nostrProfileCache[senderPubkey]
        let approval = NostrPendingApproval(
            pubkeyHex: senderPubkey,
            displayName: cached?.bestLabel,
            about: cached?.about,
            pictureURL: cached?.picture,
            content: event["content"] as? String
        )
        store.addNostrPendingApproval(approval)
        if isNew {
            Task { await NotificationService.notifyPendingApproval(pubkeyHex: senderPubkey) }
            ensureProfileFetch(for: senderPubkey, enrichApproval: true)
        }
    }

    // MARK: - Event JSON helper

    /// Re-serialise an inbound event dictionary back to canonical JSON
    /// for transcript export. Returns nil on failure — the conversation
    /// store accepts a nil `rawEventJSON` so this is best-effort.
    private func rawEventJSON(from event: [String: Any]) -> String? {
        (try? JSONSerialization.data(withJSONObject: event, options: [.sortedKeys]))
            .flatMap { String(data: $0, encoding: .utf8) }
    }

    // MARK: - NIP-42 AUTH

    /// Handles `["AUTH", <challenge>]` from the relay: signs a kind:22242
    /// event tagged with the relay URL and challenge, sends it as
    /// `["AUTH", <event>]`, and tracks the event id so an accepted OK can
    /// re-issue the REQ (auth-gated relays silently drop pre-auth subs).
    private func handleAuthChallenge(array: [Any]) {
        guard array.count >= 2, let challenge = array[1] as? String else {
            NostrRelayService.logger.notice("AUTH: dropped malformed challenge frame")
            return
        }
        guard let relayURL = connectedRelayURL else {
            NostrRelayService.logger.notice("AUTH: no relay URL on record; skipping")
            return
        }
        NostrRelayService.logger.notice("AUTH: challenge=\(challenge.prefix(12), privacy: .public)")
        Task { [weak self] in
            // Local-only signer for now; mirrors the convention already used by
            // `publishAgentProfileIfPossible`. A remote (NIP-46) signer would
            // require plumbing through `UserIdentityStore.signer`.
            guard let privKey = try? NostrCredentialStore.privateKey() else {
                NostrRelayService.logger.notice("AUTH: no local private key; cannot respond")
                return
            }
            guard let pair = try? NostrKeyPair(privateKeyHex: privKey) else { return }
            let draft = NostrEventDraft(
                kind: NostrProtocol.kindAuth,
                content: "",
                tags: [["relay", relayURL], ["challenge", challenge]]
            )
            guard let event = try? await LocalKeySigner(keyPair: pair).sign(draft) else { return }
            self?.sendAuthEvent(event)
        }
    }

    /// Handles relay `OK` frames so we can re-issue the REQ after a kind:22242
    /// AUTH event we sent gets accepted.
    private func handleOK(array: [Any]) {
        guard array.count >= 3,
              let eventID = array[1] as? String,
              let accepted = array[2] as? Bool,
              pendingAuthEventIDs.remove(eventID) != nil else { return }
        guard accepted else {
            NostrRelayService.logger.error("AUTH rejected by relay for event \(eventID.prefix(12), privacy: .public)")
            return
        }
        guard let pubkey = currentAgentPubkey else { return }
        NostrRelayService.logger.notice("AUTH accepted; re-issuing REQ for \(pubkey.prefix(12), privacy: .public)")
        sendSubscription(agentPubkey: pubkey)
    }

    private func sendAuthEvent(_ event: SignedNostrEvent) {
        let eventDict: [String: Any] = [
            "id": event.id,
            "pubkey": event.pubkey,
            "created_at": event.created_at,
            "kind": event.kind,
            "tags": event.tags,
            "content": event.content,
            "sig": event.sig,
        ]
        let frame: [Any] = [NostrProtocol.authMessage, eventDict]
        do {
            let data = try JSONSerialization.data(withJSONObject: frame)
            guard let text = String(data: data, encoding: .utf8) else { return }
            pendingAuthEventIDs.insert(event.id)
            webSocketTask?.send(.string(text)) { error in
                if let error {
                    NostrRelayService.logger.error("AUTH send failed: \(error, privacy: .public)")
                } else {
                    NostrRelayService.logger.notice("AUTH sent (event \(event.id.prefix(12), privacy: .public))")
                }
            }
        } catch {
            NostrRelayService.logger.error("AUTH serialization failed: \(error, privacy: .public)")
        }
    }

    // MARK: - Profile fetching

    private func ensureProfileFetch(for pubkey: String, enrichApproval: Bool = false) {
        if store.state.nostrProfileCache[pubkey] != nil, !enrichApproval { return }
        guard !profileFetchInflight.contains(pubkey) else { return }
        profileFetchInflight.insert(pubkey)
        Task { [weak self] in
            guard let self else { return }
            await self.profileFetcher.fetchProfiles(for: [pubkey])
            self.profileFetchInflight.remove(pubkey)
            if enrichApproval, let profile = self.store.state.nostrProfileCache[pubkey] {
                self.store.enrichNostrPendingApproval(pubkeyHex: pubkey, from: profile)
            }
        }
    }
}
