import SwiftUI

// MARK: - EpisodeCommentsSection
//
// Embedded comments thread for an episode. Hosts:
//   - A live list of NIP-22 (kind 1111) comments anchored to the Podcasting
//     2.0 `<podcast:guid>` via NIP-73 (`podcast:item:guid:<guid>`).
//   - A composer that publishes through the kernel, which signs with the
//     active user signer and routes through its relay pool (no iOS WebSocket,
//     no secret bytes in app code).
//
// This is a section, not a sheet — it lives inside the existing
// `EpisodeDetailView` scroll, so the user discovers comments in the same
// surface they're already on. Phase 1 supports top-level comments only;
// reply threading + reactions are deliberate future work.
//
// Comments live entirely in Nostr — there's no local persistence. The kernel
// owns the comment cache and projects it onto `PodcastUpdate.comments` for the
// episode the user is currently viewing; the view reads that projection
// reactively (no polling, no app-side subscription lifecycle).
struct EpisodeCommentsSection: View {

    let episode: Episode

    @Environment(AppStateStore.self) private var store
    private var identity: UserIdentityStore { store.identity }

    @State private var draft: String = ""
    @FocusState private var composerFocused: Bool

    /// Comments for this episode, projected by the kernel onto the snapshot.
    /// The kernel scopes `comments` to the episode whose `fetch_comments` was
    /// last dispatched (see `kernelFetchComments`), so the list is already the
    /// right episode's thread.
    private var comments: [CommentSummary] {
        store.kernel?.podcastSnapshot?.comments ?? []
    }

    /// Comments can only be anchored when the episode carries a publisher GUID
    /// (Podcasting 2.0 `<guid>` element). Episodes without a GUID can't be
    /// globally addressed on Nostr, so we hide the surface rather than fake a
    /// key.
    private var hasGUID: Bool {
        !episode.guid.isEmpty
    }

    var body: some View {
        // Parent (EpisodeDetailHeroView) already pads horizontally — no
        // own padding here, otherwise the section sits in a narrower
        // gutter than the show notes immediately above it.
        Group {
            if hasGUID {
                content
            } else {
                unsupportedState
            }
        }
    }

    // MARK: - Content

    private var content: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
            header
            composer
            commentsList
        }
        .task(id: episode.id) {
            store.kernelFetchComments(episodeID: episode.id.uuidString.lowercased())
        }
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
                    post()
                } label: {
                    Text("Post")
                        .font(.subheadline.weight(.semibold))
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
        if let display = identity.npubShort ?? identity.publicKeyHex.map(Self.shortKey) {
            HStack(spacing: 4) {
                Image(systemName: "person.crop.circle.fill")
                    .foregroundStyle(.secondary)
                Text(display)
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
        !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
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

    private func commentRow(_ comment: CommentSummary) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: AppTheme.Spacing.xs) {
                Text(comment.authorName ?? Self.shortKey(comment.authorNpub))
                    .font(.caption.monospaced().weight(.semibold))
                    .foregroundStyle(.primary)
                Text("·")
                    .foregroundStyle(.tertiary)
                Text(
                    Date(timeIntervalSince1970: TimeInterval(comment.createdAt)),
                    style: .relative
                )
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

    // MARK: - Actions

    private func post() {
        guard hasGUID, canPublish else { return }
        let text = draft
        store.kernelPostComment(episodeID: episode.id.uuidString.lowercased(), content: text)
        draft = ""
        composerFocused = false
        Haptics.success()
    }

    // MARK: - Helpers

    /// Display label for an npub — last 8 characters, matching the compact
    /// affordance used elsewhere in the app.
    private static func shortKey(_ key: String) -> String {
        String(key.suffix(8))
    }
}
