import SwiftUI

// MARK: - ContactRow
//
// One avatar tile in the `SocialView` "Following" grid. Driven by a single
// `ContactSummary` row from the kernel `social.following` projection — no
// fetch logic, no local caching. The grid passes a fixed `avatarSize` so
// every cell renders identically regardless of metadata presence.

/// Renders one contact avatar with the display name (or truncated npub
/// fallback) below it. Used inside the `SocialView` grid.
struct ContactRow: View {
    let contact: ContactSummary
    /// Edge length of the circular avatar in points. Defaults match the
    /// adaptive grid minimum so cells stay balanced across iPhone widths.
    var avatarSize: CGFloat = 72

    var body: some View {
        VStack(spacing: AppTheme.Spacing.sm) {
            avatar
            Text(label)
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.primary)
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(maxWidth: .infinity)
                .multilineTextAlignment(.center)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
    }

    // MARK: Avatar

    @ViewBuilder
    private var avatar: some View {
        ZStack {
            Circle()
                .fill(Color.accentColor.opacity(0.18))
            if let urlStr = contact.pictureUrl, let url = URL(string: urlStr) {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFill()
                    default:
                        initialView
                    }
                }
                .clipShape(Circle())
            } else {
                initialView
            }
        }
        .frame(width: avatarSize, height: avatarSize)
        .overlay(
            Circle().strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
        )
    }

    @ViewBuilder
    private var initialView: some View {
        if let initial = initialCharacter {
            Text(initial)
                .font(.system(size: avatarSize * 0.42, weight: .semibold))
                .foregroundStyle(.secondary)
        } else {
            Image(systemName: "person.crop.circle")
                .font(.system(size: avatarSize * 0.6))
                .foregroundStyle(.tertiary)
        }
    }

    // MARK: Label derivation

    /// Display name when set, else a truncated bech32 stub.
    private var label: String {
        if let name = contact.displayName, !name.isEmpty {
            return name
        }
        return Self.truncatedNpub(contact.npub)
    }

    /// Uppercased first letter of the display name; `nil` when no name is
    /// set so the system-symbol fallback renders instead. (We deliberately
    /// don't pull an initial from the bech32 stub — the prefix character is
    /// always `n` and would mislead the user.)
    private var initialCharacter: String? {
        guard let name = contact.displayName,
              let first = name.first(where: { $0.isLetter || $0.isNumber })
        else { return nil }
        return String(first).uppercased()
    }

    private var accessibilityLabel: String {
        if let name = contact.displayName, !name.isEmpty {
            return "\(name), Nostr contact"
        }
        return "Nostr contact \(Self.truncatedNpub(contact.npub))"
    }

    /// `npub1abcd…wxyz` — first 9 + last 4 characters, matching the truncation
    /// style the Identity surfaces use elsewhere in the app.
    static func truncatedNpub(_ npub: String) -> String {
        guard npub.count > 16 else { return npub }
        let prefix = npub.prefix(9)
        let suffix = npub.suffix(4)
        return "\(prefix)…\(suffix)"
    }
}
