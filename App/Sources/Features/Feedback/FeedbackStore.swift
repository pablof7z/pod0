import Foundation
import SwiftUI

// MARK: - FeedbackStore

/// In-app feedback threads, reactively projected from the kernel snapshot.
///
/// The Rust kernel owns the relay connection (NMP relay pool, no iOS
/// WebSocket): `kernelFetchFeedback` opens a relay-pinned subscription to the
/// feedback relay and inbound events land on `podcastSnapshot.feedbackEvents`.
/// `threads` is a computed projection over that snapshot, merged with
/// local-only state the relay can't carry: optimistic just-posted threads,
/// attached screenshots, and client-side deletions.
@MainActor
@Observable
final class FeedbackStore {

    /// Injected by the view in `.task` before the first `load()`. Weak — the
    /// view owns the store, the app owns the AppStateStore. Reading it inside
    /// `threads` registers `@Observable` tracking through to
    /// `podcastSnapshot.feedbackEvents`, so a kernel push re-renders the list.
    weak var appStore: AppStateStore?

    /// `true` after `load()` dispatches a fetch and before the first snapshot
    /// frame carrying feedback events arrives.
    var isLoading: Bool = false
    var loadError: String?

    // MARK: Local-only overlay state (never sourced from the relay)

    /// Optimistically-posted threads, shown immediately while the publish
    /// propagates and the kernel re-fetches. Dropped once the relay echo for
    /// the same author+content arrives in the snapshot.
    private var pendingThreads: [FeedbackThread] = []
    /// Optimistically-posted replies, keyed by root thread id, shown
    /// immediately while the publish propagates. The fetch is `OneShot`, so a
    /// just-posted reply isn't in the snapshot until a later fetch — without
    /// this overlay the reply would vanish until pull-to-refresh. Dropped once
    /// the relay echo (same author + content under that root) arrives.
    private var pendingReplies: [String: [FeedbackReply]] = [:]
    /// Screenshots attached to optimistic threads, keyed by thread id.
    private var attachedImages: [String: UIImage] = [:]
    /// Thread ids the user dismissed locally (client-side hide).
    private var hiddenThreadIDs: Set<String> = []
    /// Last identity pubkey seen, so `threads` can resolve "is from me".
    private var localPubkey: String?

    // MARK: Projection

    /// Reactive feedback threads. Reading this in a SwiftUI body tracks the
    /// snapshot keypath, so the next kernel push re-renders.
    var threads: [FeedbackThread] {
        let events = (appStore?.kernel?.podcastSnapshot?.feedbackEvents ?? [])
            .map(\.asSignedEvent)
        let projected = buildThreads(from: events, localPubkey: localPubkey)
        // Drop optimistic threads whose relay echo has arrived (same author +
        // content), so "Mine" doesn't show a synthetic + real duplicate row.
        let echoed = Set(projected.map { ThreadDedupKey(thread: $0) })
        let surviving = pendingThreads.filter { !echoed.contains(ThreadDedupKey(thread: $0)) }
        return (surviving + projected)
            .filter { !hiddenThreadIDs.contains($0.id) }
            .sorted { $0.createdAt > $1.createdAt }
    }

    func load(identity: UserIdentityStore) async {
        localPubkey = identity.publicKeyHex
        loadError = nil
        // Fire-and-forget: the kernel opens the subscription; events arrive on
        // the snapshot. `isLoading` clears once any feedback event is present.
        guard let appStore else {
            loadError = "Feedback is unavailable."
            return
        }
        isLoading = (appStore.kernel?.podcastSnapshot?.feedbackEvents.isEmpty ?? true)
        appStore.kernelFetchFeedback()
        if !isLoading { return }
        // Brief grace so the spinner doesn't hang forever if the relay is empty
        // (no events at all is a valid steady state). This is a UI debounce, not
        // a data poll — `threads` updates reactively regardless.
        try? await Task.sleep(for: .seconds(3))
        isLoading = false
    }

    @discardableResult
    func publishThread(
        category: FeedbackCategory,
        content: String,
        image: UIImage?,
        identity: UserIdentityStore
    ) async throws -> FeedbackThread {
        // Self-heal: ensure a local key exists (and is forwarded to the kernel
        // signer) before dispatching, mirroring `publishUserNote`. A fresh user
        // with no identity gets a generated key here.
        if !identity.hasIdentity {
            try identity._ensureGeneratedKey()
        }
        localPubkey = identity.publicKeyHex
        // Sign + publish through the kernel (NMP signs, AUTHs, routes to the
        // feedback relay). Fire-and-forget — no returned signed event.
        appStore?.kernelPublishFeedback(category: category.tagValue, content: content.trimmed)
        // Synthesize an optimistic thread from the inputs (mirrors the prior
        // returned-event shape) so the UI reflects the post immediately.
        let thread = FeedbackThread(
            authorPubkey: identity.publicKeyHex ?? "",
            category: category,
            content: content.trimmed,
            attachedImage: image
        )
        pendingThreads.insert(thread, at: 0)
        if let image { attachedImages[thread.id] = image }
        return thread
    }

    func publishReply(content: String, threadID: String, identity: UserIdentityStore) async throws {
        if !identity.hasIdentity {
            try identity._ensureGeneratedKey()
        }
        localPubkey = identity.publicKeyHex
        guard let thread = threads.first(where: { $0.id == threadID }) else { return }
        let trimmed = content.trimmed
        let lastSpeaker = thread.replies.last?.authorPubkey ?? thread.authorPubkey
        let replyToPubkey = lastSpeaker == identity.publicKeyHex ? nil : lastSpeaker
        appStore?.kernelPublishFeedback(
            category: thread.category.tagValue,
            content: trimmed,
            parentEventID: thread.eventID,
            replyToPubkey: replyToPubkey
        )
        // Optimistically show the reply immediately (the OneShot fetch won't
        // return the just-posted event yet). Keyed by the root thread id.
        let optimistic = FeedbackReply(
            authorPubkey: identity.publicKeyHex ?? "",
            content: trimmed,
            isFromMe: true
        )
        pendingReplies[thread.eventID, default: []].append(optimistic)
    }

    /// Re-open the feedback subscription (pull-to-refresh / detail appear).
    /// Replies + metadata ride the same `["a"]`-anchored fetch as roots, so
    /// there is no separate replies subscription.
    func loadReplies(for thread: FeedbackThread, identity: UserIdentityStore) async {
        localPubkey = identity.publicKeyHex
        appStore?.kernelFetchFeedback()
    }

    func deleteThread(id: String) {
        hiddenThreadIDs.insert(id)
        pendingThreads.removeAll { $0.id == id }
        pendingReplies[id] = nil
        attachedImages[id] = nil
    }

    // MARK: Thread reconstruction (unchanged from the relay-client era)

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
                var thread = FeedbackThread(
                    event: root,
                    replies: (replies[root.id] ?? []).sorted { $0.created_at < $1.created_at },
                    metadata: metaByRoot[root.id],
                    attachedImage: attachedImages[root.id],
                    localPubkey: localPubkey
                )
                // Merge optimistic replies, dropping any whose relay echo (same
                // author + content under this root) has already arrived.
                let echoed = Set(thread.replies.map { ReplyDedupKey(reply: $0) })
                let surviving = (pendingReplies[root.id] ?? [])
                    .filter { !echoed.contains(ReplyDedupKey(reply: $0)) }
                if !surviving.isEmpty {
                    thread.replies = (thread.replies + surviving)
                        .sorted { $0.createdAt < $1.createdAt }
                }
                return thread
            }
    }
}

// MARK: - Optimistic-vs-echo dedup

/// Identity used to drop an optimistic thread once its relay echo arrives.
/// Keyed by author + content so the synthetic `local-<uuid>` row and the real
/// event (with its own id) collapse to one under the "Mine" filter.
private struct ThreadDedupKey: Hashable {
    let authorPubkey: String
    let content: String
    init(thread: FeedbackThread) {
        authorPubkey = thread.authorPubkey
        content = thread.content
    }
}

/// Identity used to drop an optimistic reply once its relay echo arrives.
private struct ReplyDedupKey: Hashable {
    let authorPubkey: String
    let content: String
    init(reply: FeedbackReply) {
        authorPubkey = reply.authorPubkey
        content = reply.content
    }
}

// MARK: - Feedback relay constants
//
// Feedback publish + fetch + profile resolution all run through the Rust
// kernel now (NMP relay pool — `feedback_handler.rs`, kernel-side NIP-42 AUTH;
// profile fetch via `KernelModel.claimProfile`). No iOS WebSocket and no Swift
// signing remain for feedback. The legacy `FeedbackRelayClient` actor (its
// WebSocket REQ/AUTH/publish machinery and the `signer.sign` NIP-42 auth path)
// has been deleted. Only the wire constants survive — read by
// `FeedbackStore.buildThreads` and `LivePeerEventPublisher`.

enum FeedbackRelayClient {
    static let textNoteKind = 1
    static let metadataKind = 513
    static let projectCoordinate =
        "31933:09d48a1a5dbe13404a729634f1d6ba722d40513468dd713c8ea38ca9b7b6f2c7:podcast"
}
