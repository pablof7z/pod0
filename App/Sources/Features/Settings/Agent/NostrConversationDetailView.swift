import SwiftUI
import UIKit

/// Full transcript of a single Nostr conversation root, rendered in a
/// Slack-style layout: all messages left-aligned, with avatar + sender
/// name shown at the start of each burst (sender change or > 5 min gap).
struct NostrConversationDetailView: View {
    let conversation: NostrConversationRecord
    @State private var showExportSheet = false
    @Environment(AppStateStore.self) private var store

    private static let burstGapSeconds: TimeInterval = 300

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 2) {
                ForEach(Array(conversation.turns.enumerated()), id: \.element.eventID) { index, turn in
                    NostrSlackBubble(
                        turn: turn,
                        showHeader: showHeader(at: index),
                        profile: store.state.nostrProfileCache[turn.pubkey]
                    )
                }
            }
            .padding(AppTheme.Spacing.md)
        }
        .defaultScrollAnchor(.bottom)
        .navigationTitle("Conversation")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    Haptics.selection()
                    showExportSheet = true
                } label: {
                    Image(systemName: "square.and.arrow.up")
                }
                .accessibilityLabel("Export events")
            }
        }
        .sheet(isPresented: $showExportSheet) {
            NostrConversationExportSheet(jsonlContent: generateJSONL())
        }
    }

    private func showHeader(at index: Int) -> Bool {
        guard index > 0 else { return true }
        let prev = conversation.turns[index - 1]
        let curr = conversation.turns[index]
        if prev.pubkey != curr.pubkey { return true }
        return curr.createdAt.timeIntervalSince(prev.createdAt) > Self.burstGapSeconds
    }

    private func generateJSONL() -> String {
        conversation.turns
            .compactMap { $0.rawEventJSON }
            .joined(separator: "\n")
    }
}

// MARK: - Slack-style bubble

private struct NostrSlackBubble: View {
    let turn: NostrConversationTurn
    let showHeader: Bool
    let profile: NostrProfileMetadata?

    private enum Layout {
        static let avatarSize: CGFloat = 32
        static let bubbleCornerRadius: CGFloat = 14
        static let bubblePaddingH: CGFloat = 10
    }

    var body: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.sm) {
            avatarSlot
            VStack(alignment: .leading, spacing: 3) {
                if showHeader {
                    HStack(alignment: .firstTextBaseline, spacing: AppTheme.Spacing.xs) {
                        Text(displayName)
                            .font(AppTheme.Typography.caption.weight(.semibold))
                            .foregroundStyle(
                                turn.direction == .outgoing ? Color.accentColor : Color.primary
                            )
                        Text(timestamp)
                            .font(AppTheme.Typography.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
                Text(turn.content)
                    .font(AppTheme.Typography.body)
                    .foregroundStyle(Color.primary)
                    .multilineTextAlignment(.leading)
                    .padding(.horizontal, Layout.bubblePaddingH)
                    .padding(.vertical, AppTheme.Spacing.xs)
                    .background(
                        RoundedRectangle(cornerRadius: Layout.bubbleCornerRadius, style: .continuous)
                            .fill(turn.direction == .outgoing
                                ? AppTheme.Tint.agentSurface.opacity(0.18)
                                : Color(.secondarySystemBackground))
                    )
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 1)
    }

    @ViewBuilder
    private var avatarSlot: some View {
        if showHeader {
            NostrProfileAvatar(profile: profile)
                .frame(width: Layout.avatarSize, height: Layout.avatarSize)
        } else {
            Color.clear.frame(width: Layout.avatarSize, height: 1)
        }
    }

    private var displayName: String {
        if let label = profile?.bestLabel { return label }
        if turn.direction == .outgoing { return "Agent" }
        return NostrNpub.shortNpub(fromHex: turn.pubkey)
    }

    private var timestamp: String {
        turn.createdAt.formatted(date: .abbreviated, time: .shortened)
    }
}

// MARK: - Export sheet

struct NostrConversationExportSheet: View {
    let jsonlContent: String
    @Environment(\.dismiss) private var dismiss
    @State private var copied = false
    @State private var fileURL: URL?

    var body: some View {
        NavigationStack {
            VStack(spacing: AppTheme.Spacing.md) {
                HStack {
                    Text("Events (JSONL)")
                        .font(AppTheme.Typography.headline)
                    Spacer()
                    Button(action: copyToClipboard) {
                        Label(copied ? "Copied" : "Copy", systemImage: "doc.on.doc")
                            .foregroundStyle(copied ? AppTheme.Tint.success : Color.accentColor)
                    }
                    .disabled(copied)
                }
                .padding(.horizontal, AppTheme.Spacing.md)

                ScrollView {
                    Text(jsonlContent.isEmpty ? "(no exportable events)" : jsonlContent)
                        .font(AppTheme.Typography.mono)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(AppTheme.Spacing.md)
                        .background(
                            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                                .fill(AppTheme.Tint.surfaceMuted)
                        )
                        .padding(.horizontal, AppTheme.Spacing.md)
                }
                .frame(maxHeight: .infinity)

                shareButton
                    .padding(.horizontal, AppTheme.Spacing.md)
            }
            .padding(.vertical, AppTheme.Spacing.md)
            .navigationTitle("Export Events")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Done") { dismiss() }
                }
            }
            .onAppear { fileURL = writeTempFile() }
        }
    }

    @ViewBuilder
    private var shareButton: some View {
        if let url = fileURL {
            ShareLink(item: url, subject: Text("Conversation Events"), label: { shareLabel })
        } else {
            ShareLink(item: jsonlContent, subject: Text("Conversation Events"), label: { shareLabel })
        }
    }

    private var shareLabel: some View {
        Label("Share", systemImage: "square.and.arrow.up")
            .frame(maxWidth: .infinity)
            .padding(AppTheme.Spacing.md)
            .background(Color.accentColor)
            .foregroundStyle(.white)
            .cornerRadius(AppTheme.Corner.md)
    }

    private func writeTempFile() -> URL? {
        let suffix = UUID().uuidString.prefix(8)
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("conversation-\(suffix).jsonl")
        do {
            try jsonlContent.write(to: url, atomically: true, encoding: .utf8)
            return url
        } catch {
            return nil
        }
    }

    private func copyToClipboard() {
        UIPasteboard.general.string = jsonlContent
        copied = true
        Haptics.success()
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            copied = false
        }
    }
}
