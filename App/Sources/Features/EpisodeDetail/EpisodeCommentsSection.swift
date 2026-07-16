import SwiftUI

/// Episode discussion backed by NMP's typed NIP-22/NIP-73 module. Pod0 owns
/// presentation and durable receipt-id reattachment, while NMP owns protocol
/// composition, verification, routing, signing, relay sessions, and retries.
struct EpisodeCommentsSection: View {
    let episode: Episode

    @Environment(\.episodeCommentsRepository) private var repository

    private var target: CommentTarget? {
        let guid = episode.guid.trimmingCharacters(in: .whitespacesAndNewlines)
        return guid.isEmpty ? nil : .episode(guid: guid)
    }

    var body: some View {
        if let target {
            EpisodeCommentsLoadedSection(target: target, repository: repository)
        } else {
            HStack(spacing: AppTheme.Spacing.sm) {
                Image(systemName: "info.circle")
                    .foregroundStyle(.secondary)
                Text("This episode has no Podcasting 2.0 GUID, so comments can't be anchored.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .padding(AppTheme.Spacing.sm)
        }
    }
}

private struct EpisodeCommentsLoadedSection: View {
    let target: CommentTarget

    @State private var model: EpisodeCommentsModel
    @State private var reloadID = UUID()
    @FocusState private var composerFocused: Bool

    init(target: CommentTarget, repository: any EpisodeCommentsRepository) {
        self.target = target
        _model = State(initialValue: EpisodeCommentsModel(repository: repository))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
            header
            composer
            if let message = model.submitError {
                statusText(message, color: .red)
            }
            if let message = model.loadError {
                loadFailure(message)
            }
            commentsList
        }
        .task(id: reloadID) { await model.observe(target: target) }
    }

    private var header: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: "bubble.left.and.text.bubble.right")
                .foregroundStyle(.secondary)
            Text("Comments")
                .font(.headline)
            if !model.comments.isEmpty {
                Text("\(model.comments.count)")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 2)
                    .background(.secondary.opacity(0.12), in: .capsule)
            }
            Spacer()
            acquisitionLabel
        }
    }

    @ViewBuilder
    private var acquisitionLabel: some View {
        if model.isLoading {
            ProgressView()
                .controlSize(.small)
                .accessibilityLabel("Loading comments")
        } else if model.acquisition.connectedSourceCount > 0 {
            Text("\(model.acquisition.connectedSourceCount) live")
                .font(.caption)
                .foregroundStyle(.secondary)
        } else if model.acquisition.sourceCount > 0 {
            Text("Cached")
                .font(.caption)
                .foregroundStyle(.secondary)
        } else if model.acquisition.hasShortfall {
            Text("No relay route")
                .font(.caption)
                .foregroundStyle(.orange)
        }
    }

    private var composer: some View {
        VStack(alignment: .trailing, spacing: AppTheme.Spacing.xs) {
            TextField("Add a comment…", text: $model.draft, axis: .vertical)
                .textFieldStyle(.plain)
                .focused($composerFocused)
                .lineLimit(1...4)
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.sm)
                .glassEffect(.regular, in: .rect(cornerRadius: AppTheme.Corner.md))
            HStack {
                identityChip
                Spacer()
                Button("Post") { submit() }
                    .font(.subheadline.weight(.semibold))
                    .buttonStyle(.borderedProminent)
                    .disabled(!model.canSubmit)
                    .overlay {
                        if model.isSubmitting {
                            ProgressView().controlSize(.small)
                        }
                    }
                    .opacity(model.isSubmitting ? 0.7 : 1)
            }
        }
    }

    @ViewBuilder
    private var identityChip: some View {
        if let pubkey = model.activeAuthorPubkey {
            HStack(spacing: 4) {
                Image(systemName: "person.crop.circle.fill")
                    .foregroundStyle(.secondary)
                Text(shortKey(pubkey))
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
            }
        } else {
            Text("Nostr signing identity unavailable")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var commentsList: some View {
        if model.comments.isEmpty && model.outgoing.isEmpty && model.loadError == nil && !model.isLoading {
            Text("Be the first to comment. Published comments remain portable across NIP-22 clients.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .padding(.vertical, AppTheme.Spacing.sm)
        } else {
            VStack(spacing: AppTheme.Spacing.sm) {
                ForEach(model.outgoing) { outgoingRow($0) }
                ForEach(model.comments) { commentRow($0) }
            }
        }
    }

    private func commentRow(_ comment: EpisodeComment) -> some View {
        commentCard(
            author: comment.authorShortKey,
            content: comment.content,
            date: comment.createdAt,
            status: nil,
            statusColor: .secondary
        )
    }

    private func outgoingRow(_ comment: OutgoingEpisodeComment) -> some View {
        let isFailure: Bool
        switch comment.phase {
        case .failed, .deliveryUnknown: isFailure = true
        default: isFailure = false
        }
        return commentCard(
            author: "You",
            content: comment.content,
            date: comment.submittedAt,
            status: comment.phase.label,
            statusColor: isFailure ? .red : .secondary
        )
    }

    private func commentCard(
        author: String,
        content: String,
        date: Date,
        status: String?,
        statusColor: Color
    ) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: AppTheme.Spacing.xs) {
                Text(author)
                    .font(.caption.monospaced().weight(.semibold))
                Text("·").foregroundStyle(.tertiary)
                Text(date, style: .relative)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
            }
            Text(content)
                .font(.body)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let status {
                Text(status)
                    .font(.caption)
                    .foregroundStyle(statusColor)
            }
        }
        .padding(AppTheme.Spacing.sm)
        .background(Color(.secondarySystemBackground), in: .rect(cornerRadius: AppTheme.Corner.md))
    }

    private func loadFailure(_ message: String) -> some View {
        HStack {
            statusText(message, color: .red)
            Spacer()
            Button("Retry") { reloadID = UUID() }
                .font(.caption.weight(.semibold))
        }
    }

    private func statusText(_ message: String, color: Color) -> some View {
        Text(message)
            .font(.caption)
            .foregroundStyle(color)
            .padding(.horizontal, AppTheme.Spacing.sm)
    }

    private func submit() {
        Task {
            await model.submit(target: target)
            if model.submitError == nil {
                composerFocused = false
                Haptics.success()
            } else {
                Haptics.error()
            }
        }
    }

    private func shortKey(_ pubkey: String) -> String {
        guard pubkey.count > 8 else { return pubkey }
        return "\(pubkey.prefix(4))…\(pubkey.suffix(4))"
    }
}
