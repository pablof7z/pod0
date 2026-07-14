import SwiftUI

// MARK: - EpisodeCommentsSection
//
// Embedded comments thread for an episode. Hosts:
//   - A live list of NIP-22 (kind 1111) comments anchored to the Podcasting
//     2.0 `<podcast:guid>` via NIP-73 (`podcast:item:guid:<guid>`).
//   - A composer that publishes via the user's configured signer
//     (`UserIdentityStore.signer`) to the user's configured Nostr relay.
//
// This is a section, not a sheet — it lives inside the existing
// `EpisodeDetailView` scroll, so the user discovers comments in the same
// surface they're already on. Phase 1 supports top-level comments only;
// reply threading + reactions are deliberate future work.
//
// Comments live entirely in Nostr — there's no local persistence. A
// dismissed view re-fetches on the next appear from the relay's index.
// That's fine for v1 and matches Fountain's behaviour.
struct EpisodeCommentsSection: View {

    let episode: Episode

    @Environment(AppStateStore.self) private var store
    @Environment(UserIdentityStore.self) private var identity

    /// Live websocket subscription. Held in @State so the view's lifetime
    /// drives the network connection's lifetime.
    @State private var subscription: NostrCommentService.Subscription?
    @State private var comments: [EpisodeComment] = []
    @State private var draft: String = ""
    @State private var isPublishing = false
    @State private var errorMessage: String?
    @FocusState private var composerFocused: Bool

    /// The comment target — only resolves when the episode carries a
    /// publisher GUID (Podcasting 2.0 `<guid>` element). Episodes without a
    /// GUID can't be globally addressed on Nostr, so we hide the surface
    /// rather than fake a key.
    private var target: CommentTarget? {
        guard !episode.guid.isEmpty else { return nil }
        return .episode(guid: episode.guid)
    }

    var body: some View {
        // Parent (EpisodeDetailHeroView) already pads horizontally — no
        // own padding here, otherwise the section sits in a narrower
        // gutter than the show notes immediately above it.
        Group {
            if let target {
                content(target: target)
            } else {
                unsupportedState
            }
        }
    }

    // MARK: - Content

    @ViewBuilder
    private func content(target: CommentTarget) -> some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
            header
            composer
            if let errorMessage {
                Text(errorMessage)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .padding(.horizontal, AppTheme.Spacing.sm)
            }
            commentsList
        }
        .task(id: target) { await openSubscription(target: target) }
    }

    private var header: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: "bubble.left.and.text.bubble.right")
                .foregroundStyle(.secondary)
            Text("Comments")
                .font(.headline)
            if !comments.isEmpty {
                Text("\(comments.count)")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 2)
                    .background(.secondary.opacity(0.12), in: .capsule)
            }
            Spacer()
        }
    }

    // MARK: - Composer

    private var composer: some View {
        VStack(alignment: .trailing, spacing: AppTheme.Spacing.xs) {
            TextField("Add a comment…", text: $draft, axis: .vertical)
                .textFieldStyle(.plain)
                .focused($composerFocused)
                .lineLimit(1...4)
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.sm)
                .glassEffect(.regular, in: .rect(cornerRadius: AppTheme.Corner.md))
            HStack {
                identityChip
                Spacer()
                Button {
                    Task { await publish() }
                } label: {
                    if isPublishing {
                        ProgressView().controlSize(.small)
                    } else {
                        Text("Post")
                            .font(.subheadline.weight(.semibold))
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(!canPublish)
            }
        }
    }

    /// Short identity affordance — shows the npub stub the comment will
    /// post under, or a prompt to set up Nostr if no signer is configured.
    @ViewBuilder
    private var identityChip: some View {
        if let pubkey = identity.publicKeyHex {
            HStack(spacing: 4) {
                Image(systemName: "person.crop.circle.fill")
                    .foregroundStyle(.secondary)
                Text(EpisodeComment(
                    id: "",
                    target: .episode(guid: ""),
                    authorPubkeyHex: pubkey,
                    content: "",
                    createdAt: Date()
                ).authorShortKey)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
            }
        } else {
            Text("Nostr key not set up")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var canPublish: Bool {
        guard !isPublishing else { return false }
        guard identity.signer != nil else { return false }
        return !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    // MARK: - List

    @ViewBuilder
    private var commentsList: some View {
        if comments.isEmpty {
            Text("Be the first to comment. Posts publish to your Nostr relay and stay readable from any NIP-22 client.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .padding(.vertical, AppTheme.Spacing.sm)
        } else {
            VStack(spacing: AppTheme.Spacing.sm) {
                ForEach(comments) { comment in
                    commentRow(comment)
                }
            }
        }
    }

    private func commentRow(_ comment: EpisodeComment) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: AppTheme.Spacing.xs) {
                Text(comment.authorShortKey)
                    .font(.caption.monospaced().weight(.semibold))
                    .foregroundStyle(.primary)
                Text("·")
                    .foregroundStyle(.tertiary)
                Text(comment.createdAt, style: .relative)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
            }
            Text(comment.content)
                .font(.body)
                .foregroundStyle(.primary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(AppTheme.Spacing.sm)
        .background(
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .fill(Color(.secondarySystemBackground))
        )
    }

    // MARK: - Unsupported

    private var unsupportedState: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: "info.circle")
                .foregroundStyle(.secondary)
            Text("This episode has no Podcasting 2.0 GUID, so comments can't be anchored. Ask the publisher to add a <podcast:guid> element.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(AppTheme.Spacing.sm)
    }

    // MARK: - Network lifecycle

    private func openSubscription(target: CommentTarget) async {
        // Tear down any prior subscription before opening a new one — the
        // task is keyed on `target`, so this branch only runs when the
        // target changed (different episode).
        subscription?.cancel()
        comments = []
        let service = NostrCommentService(store: store)
        let sub = service.subscribe(target: target)
        subscription = sub
        for await comment in sub.stream {
            // Newest first. Comments arrive out of order from the relay
            // (stored events come oldest-first; live events come in
            // creation order) — inserting at the head and re-sorting is
            // O(N log N) per arrival but N is small for episode comments.
            comments.append(comment)
            comments.sort { $0.createdAt > $1.createdAt }
        }
    }

    private func publish() async {
        guard let target,
              let signer = identity.signer,
              canPublish else { return }
        let text = draft
        isPublishing = true
        errorMessage = nil
        defer { isPublishing = false }
        do {
            let service = NostrCommentService(store: store)
            let event = try await service.publish(
                content: text,
                target: target,
                signer: signer
            )
            // Optimistically append so the user sees their comment land
            // before the relay echo round-trips through the subscription.
            let mine = EpisodeComment(
                id: event.id,
                target: target,
                authorPubkeyHex: event.pubkey,
                content: event.content,
                createdAt: Date(timeIntervalSince1970: TimeInterval(event.created_at))
            )
            if !comments.contains(where: { $0.id == mine.id }) {
                comments.insert(mine, at: 0)
            }
            draft = ""
            composerFocused = false
            Haptics.success()
        } catch {
            errorMessage = error.localizedDescription
            Haptics.error()
        }
    }
}
