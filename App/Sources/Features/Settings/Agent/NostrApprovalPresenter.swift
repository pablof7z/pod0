import SwiftUI

/// Watches `nostrPendingApprovals` and auto-presents a trust-this-user
/// sheet whenever an event from an un-trusted pubkey lands. Mounted once
/// on `RootView` so the prompt can interrupt any tab the user is on.
///
/// The sheet shows the oldest pending approval first and stays open
/// until the queue drains — each Allow / Block / Dismiss decision pops
/// the head and reveals the next pending one.
struct NostrApprovalPresenter: ViewModifier {

    @Environment(AppStateStore.self) private var store

    func body(content: Content) -> some View {
        content
            .sheet(isPresented: Binding(
                get: { !store.pendingNostrApprovals.isEmpty },
                set: { _ in /* user-driven dismissal only */ }
            )) {
                if let approval = nextApproval() {
                    NostrApprovalSheet(approval: approval)
                        .presentationDetents([.medium, .large])
                        .presentationDragIndicator(.visible)
                        .interactiveDismissDisabled(true)
                }
            }
    }

    private func nextApproval() -> NostrPendingApproval? {
        store.pendingNostrApprovals
            .sorted { $0.receivedAt < $1.receivedAt }
            .first
    }
}

extension View {
    /// Attaches the global Nostr trust-this-user sheet. Mount once high in
    /// the view hierarchy (e.g. `RootView`) so any tab can be interrupted
    /// when a new message from an unknown peer arrives.
    func nostrApprovalPresenter() -> some View {
        modifier(NostrApprovalPresenter())
    }
}

// MARK: - Sheet

private struct NostrApprovalSheet: View {
    @Environment(AppStateStore.self) private var store
    let approval: NostrPendingApproval

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
            header
            senderRow
            messageRow
            Spacer(minLength: AppTheme.Spacing.sm)
            actionButtons
        }
        .padding(AppTheme.Spacing.lg)
    }

    // MARK: - Sections

    private var header: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: "person.crop.circle.badge.questionmark")
                .font(.system(size: 28))
                .foregroundStyle(AppTheme.Tint.warning)
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                Text("New Nostr message")
                    .font(AppTheme.Typography.headline)
                Text("From a pubkey you haven't approved")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
    }

    private var senderRow: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            Text("Sender")
                .font(AppTheme.Typography.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            if let name = approval.displayName, !name.isEmpty {
                Text(name)
                    .font(AppTheme.Typography.body.weight(.semibold))
            }
            Text(NostrNpub.encode(fromHex: approval.pubkeyHex))
                .font(AppTheme.Typography.mono)
                .foregroundStyle(.secondary)
                .lineLimit(2)
                .truncationMode(.middle)
            if let about = approval.about, !about.isEmpty {
                Text(about)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
            }
        }
    }

    private var messageRow: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            Text("Message")
                .font(AppTheme.Typography.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            ScrollView {
                Text(displayContent)
                    .font(AppTheme.Typography.body)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxHeight: 200)
            .padding(AppTheme.Spacing.sm)
            .background(
                RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                    .fill(AppTheme.Tint.surfaceMuted)
            )
        }
    }

    private var actionButtons: some View {
        HStack(spacing: AppTheme.Spacing.md) {
            Button {
                store.blockNostrPubkey(approval.pubkeyHex)
                Haptics.selection()
            } label: {
                Label("Block", systemImage: "hand.raised")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .tint(AppTheme.Tint.error)

            Button {
                store.allowNostrPubkey(approval.pubkeyHex)
                Haptics.success()
            } label: {
                Label("Allow", systemImage: "checkmark.circle.fill")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.glassProminent)
            .tint(AppTheme.Tint.success)
        }
    }

    // MARK: - Derived

    private var displayContent: String {
        let trimmed = (approval.content ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "(no message content)" : trimmed
    }
}
