import SwiftUI

// MARK: - EpisodeCommentsSheet
//
// Modal sheet attached to an episode that surfaces NIP-22 (kind 1111)
// comments backed by `model.podcastSnapshot?.comments`. The kernel decides what
// to surface (D7) — this view only renders the list, composes the
// outgoing draft, and dispatches `podcast.fetch_comments` /
// `podcast.post_comment` actions.
//
// The comments list is empty for this PR — the Rust handler is a stub
// pending the relay subscription follow-up (`docs/BACKLOG.md` →
// `pr-episode-comments-relay-wiring`). The empty-state copy here is the
// long-term render once the projection lands.
struct EpisodeCommentsSheet: View {
    let episodeId: String
    let onDismiss: () -> Void

    @Environment(KernelModel.self) private var model

    @State private var draft: String = ""
    @State private var isPosting: Bool = false
    @State private var postErrorMessage: String?
    @FocusState private var composerFocused: Bool

    /// Newest-first ordering — the Rust projection sorts on its side,
    /// but iOS re-sorts defensively so an out-of-order partial tick
    /// never flips the list mid-scroll.
    private var comments: [CommentSummary] {
        (model.podcastSnapshot?.comments ?? []).sorted { $0.createdAt > $1.createdAt }
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                commentsList
                Divider()
                composer
            }
            .navigationTitle("Comments")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbar }
        }
        .onAppear(perform: fetchComments)
    }

    // MARK: - Toolbar

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                Haptics.light()
                onDismiss()
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.title3)
                    .foregroundStyle(.secondary)
            }
            .accessibilityLabel("Close comments")
        }
    }

    // MARK: - List

    @ViewBuilder
    private var commentsList: some View {
        if comments.isEmpty {
            emptyState
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
                    ForEach(comments) { comment in
                        commentRow(comment)
                    }
                }
                .padding(.horizontal, AppTheme.Spacing.lg)
                .padding(.vertical, AppTheme.Spacing.md)
            }
        }
    }

    private var emptyState: some View {
        VStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: "bubble.left.and.text.bubble.right")
                .font(.system(size: 36, weight: .light))
                .foregroundStyle(.secondary)
            Text("No comments yet.")
                .font(AppTheme.Typography.headline)
                .foregroundStyle(.primary)
            Text("Be the first to share your thoughts on this episode.")
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(AppTheme.Spacing.lg)
    }

    private func commentRow(_ comment: CommentSummary) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: AppTheme.Spacing.xs) {
                Text(authorDisplayLabel(for: comment))
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                Text("·")
                    .foregroundStyle(.tertiary)
                Text(relativeDate(from: comment.createdAt))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
            }
            Text(comment.content)
                .font(.body)
                .foregroundStyle(.primary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .textSelection(.enabled)
        }
        .padding(AppTheme.Spacing.sm)
        .background(
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .fill(Color(.secondarySystemBackground))
        )
    }

    // MARK: - Composer

    private var composer: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            if let postErrorMessage {
                Text(postErrorMessage)
                    .font(.caption)
                    .foregroundStyle(.red)
            }
            HStack(alignment: .bottom, spacing: AppTheme.Spacing.sm) {
                TextField("Add a comment…", text: $draft, axis: .vertical)
                    .textFieldStyle(.plain)
                    .focused($composerFocused)
                    .lineLimit(1...4)
                    .padding(.horizontal, AppTheme.Spacing.md)
                    .padding(.vertical, AppTheme.Spacing.sm)
                    .background(
                        RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                            .fill(Color(.tertiarySystemBackground))
                    )
                Button(action: postComment) {
                    if isPosting {
                        ProgressView().controlSize(.small)
                    } else {
                        Text("Post")
                            .font(.subheadline.weight(.semibold))
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(!canPost)
            }
        }
        .padding(.horizontal, AppTheme.Spacing.lg)
        .padding(.vertical, AppTheme.Spacing.md)
        .background(.bar)
    }

    private var canPost: Bool {
        guard !isPosting else { return false }
        return !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    // MARK: - Dispatch

    private func fetchComments() {
        model.dispatch(
            namespace: "podcast",
            body: ["op": "fetch_comments", "episode_id": episodeId]
        )
    }

    private func postComment() {
        let trimmed = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        isPosting = true
        postErrorMessage = nil
        let result = model.dispatch(
            namespace: "podcast",
            body: ["op": "post_comment",
                   "episode_id": episodeId,
                   "content": trimmed]
        )
        isPosting = false
        switch result {
        case .accepted:
            // Stub handler returns `nostr_relay_pending`; clear the
            // draft so the user sees the input reset even though the
            // optimistic comment doesn't land in the list yet. The
            // relay-wiring follow-up will surface the actual post.
            draft = ""
            composerFocused = false
            Haptics.success()
        case .failure(let message):
            postErrorMessage = message
            Haptics.warning()
        }
    }

    // MARK: - Formatting

    /// Prefer cached display name when available; fall back to the
    /// truncated npub stub (`npub1abcd…wxyz`).
    private func authorDisplayLabel(for comment: CommentSummary) -> String {
        if let name = comment.authorName, !name.isEmpty { return name }
        return truncatedNpub(comment.authorNpub)
    }

    private func truncatedNpub(_ npub: String) -> String {
        guard npub.count > 14 else { return npub }
        let prefix = npub.prefix(8)
        let suffix = npub.suffix(4)
        return "\(prefix)…\(suffix)"
    }

}
