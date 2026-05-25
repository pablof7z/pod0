import SwiftUI

// MARK: - NostrDiscoverRow

/// Row in the Nostr discovery results list. Artwork left, title +
/// author-pubkey-prefix right, subscribe button trailing.
///
/// Mirrors `DiscoverResultRow` but renders the pubkey-prefix as the
/// secondary line instead of an iTunes-style author string (Nostr
/// `kind:10154` events don't carry a human-readable host name; the
/// pubkey is the closest stable identifier).
///
/// - Tapping anywhere on the row body calls `onSubscribe` when the
///   feed_url is present.
/// - When already subscribed, a checkmark replaces the ⊕ button.
/// - When `feedUrl` is missing the row is greyed out and the subscribe
///   button is replaced with a "no feed" icon (the AddShowSheet sub-view
///   already surfaces the broader error message at the top).
struct NostrDiscoverRow: View {

    let result: NostrShowSummary
    let isSubscribing: Bool
    let isAlreadySubscribed: Bool
    let onSubscribe: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            artwork
            VStack(alignment: .leading, spacing: 2) {
                Text(result.title)
                    .font(AppTheme.Typography.headline)
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                Text(abbreviatedPubkey)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                if let description = result.description, !description.isEmpty {
                    Text(description)
                        .font(AppTheme.Typography.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .padding(.top, 2)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            trailingControl
                .padding(.top, 2)
        }
        .padding(.vertical, AppTheme.Spacing.sm)
        .contentShape(Rectangle())
        .onTapGesture { handleRowTap() }
        .opacity(isRowDisabled ? 0.65 : 1)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(.isButton)
    }

    private var isRowDisabled: Bool {
        isSubscribing || isAlreadySubscribed || result.feedUrl == nil
    }

    /// Show the first 12 hex characters of the author pubkey, prefixed
    /// with `npub-style` shorthand. Full npub bech32 conversion belongs
    /// to the future identity-display utility; this is enough to tell
    /// rows apart visually.
    private var abbreviatedPubkey: String {
        let prefix = result.authorPubkey.prefix(12)
        return "by \(prefix)…"
    }

    private var artwork: some View {
        Group {
            if let urlStr = result.artworkUrl, let url = URL(string: urlStr) {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFill()
                    default:
                        artworkPlaceholder
                    }
                }
            } else {
                artworkPlaceholder
            }
        }
        .frame(width: 64, height: 64)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous))
    }

    private var artworkPlaceholder: some View {
        ZStack {
            Color(.tertiarySystemFill)
            Image(systemName: "antenna.radiowaves.left.and.right").foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var trailingControl: some View {
        if isSubscribing {
            ProgressView()
                .controlSize(.small)
                .frame(width: 32, height: 32)
        } else if isAlreadySubscribed {
            Image(systemName: "checkmark.circle.fill")
                .font(.title3)
                .foregroundStyle(.secondary)
                .frame(width: 32, height: 32)
        } else if result.feedUrl == nil {
            Image(systemName: "link.badge.plus")
                .font(.title3)
                .foregroundStyle(.tertiary)
                .frame(width: 32, height: 32)
                .accessibilityLabel("No RSS feed available")
        } else {
            Image(systemName: "plus.circle.fill")
                .font(.title3)
                .foregroundStyle(.tint)
                .frame(width: 32, height: 32)
        }
    }

    private func handleRowTap() {
        guard !isRowDisabled else { return }
        onSubscribe()
    }

    private var accessibilityLabel: String {
        if isAlreadySubscribed { return "Already subscribed to \(result.title)" }
        if isSubscribing { return "Subscribing to \(result.title)" }
        if result.feedUrl == nil {
            return "\(result.title) — no RSS feed available, cannot subscribe yet"
        }
        return "Subscribe to \(result.title)"
    }
}
