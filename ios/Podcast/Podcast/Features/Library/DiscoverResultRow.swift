import SwiftUI

// MARK: - DiscoverResultRow

/// Row in the directory search results. Artwork left, title + author + meta
/// in the middle, subscribe button right.
///
/// **Tap behaviour:**
///   - Tapping anywhere on the row body triggers `onSubscribe` (so the
///     whole row reads as a tap target). The trailing ⊕ button does the
///     same thing — it's mostly a visual affordance.
///   - When a previous subscribe attempt failed for this row, a red ⚠
///     chip replaces the ⊕. Tapping the chip toggles a one-line error
///     caption directly underneath the row.
///   - When already subscribed, both surfaces become a non-interactive
///     checkmark.
struct DiscoverResultRow: View {

    let result: ITunesSearchClient.Result
    let isSubscribing: Bool
    let isAlreadySubscribed: Bool
    /// Last per-row subscribe failure for this result, or `nil` when the
    /// row is in a normal / success state.
    let rowError: String?
    /// Whether the inline error caption is currently expanded.
    let isErrorExpanded: Bool
    let onSubscribe: () -> Void
    let onToggleErrorExpansion: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            rowBody
            if rowError != nil, isErrorExpanded {
                errorCaption
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .animation(AppTheme.Animation.springFast, value: isErrorExpanded)
    }

    // MARK: - Subviews

    private var rowBody: some View {
        // Plain HStack + onTapGesture rather than a wrapping Button, so the
        // trailing chip (which is itself a Button when an error is shown)
        // doesn't have to compete with an outer Button for the tap. The
        // trailing chip's Button always wins for taps on its hit area; the
        // surrounding row receives taps everywhere else.
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            artwork
            VStack(alignment: .leading, spacing: 2) {
                Text(result.collectionName)
                    .font(AppTheme.Typography.headline)
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                if let artist = result.artistName, !artist.isEmpty {
                    Text(artist)
                        .font(AppTheme.Typography.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                metaRow
                    .padding(.top, 2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            trailingControl
                .padding(.top, 2)
        }
        .padding(.vertical, AppTheme.Spacing.sm)
        .contentShape(Rectangle())
        .onTapGesture { handleRowTap() }
        .opacity(isRowTapDisabled ? 0.65 : 1)
        .allowsHitTesting(true)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(.isButton)
    }

    /// Whether the row body itself should dim / no-op when tapped.
    /// (The trailing chip remains tappable in error / subscribed states
    /// for retry / accessibility purposes.)
    private var isRowTapDisabled: Bool {
        isSubscribing || isAlreadySubscribed
    }

    private var artwork: some View {
        CachedAsyncImage(url: result.artworkURL, targetSize: CGSize(width: 64, height: 64)) { phase in
            switch phase {
            case .success(let image):
                image.resizable().aspectRatio(contentMode: .fill)
            case .empty, .failure:
                ZStack {
                    Color(.tertiarySystemFill)
                    Image(systemName: "waveform")
                        .foregroundStyle(.secondary)
                }
            @unknown default:
                Color(.tertiarySystemFill)
            }
        }
        .frame(width: 64, height: 64)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous))
    }

    @ViewBuilder
    private var metaRow: some View {
        let bits: [String] = {
            var parts: [String] = []
            if let g = result.primaryGenreName, !g.isEmpty { parts.append(g) }
            if let count = result.trackCount, count > 0 {
                parts.append("\(count) episode\(count == 1 ? "" : "s")")
            }
            return parts
        }()
        if !bits.isEmpty {
            Text(bits.joined(separator: " · "))
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
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
        } else if rowError != nil {
            // Wrap the chip in its own button so the gesture doesn't
            // bubble to the row-level subscribe action — tapping the
            // ⚠ should expand the error, not retry.
            Button(action: onToggleErrorExpansion) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.title3)
                    .foregroundStyle(AppTheme.Tint.error)
                    .frame(width: 32, height: 32)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Show error for \(result.collectionName)")
        } else {
            Image(systemName: "plus.circle.fill")
                .font(.title3)
                .foregroundStyle(.tint)
                .frame(width: 32, height: 32)
        }
    }

    private var errorCaption: some View {
        Label(rowError ?? "", systemImage: "exclamationmark.triangle.fill")
            .font(AppTheme.Typography.caption)
            .foregroundStyle(AppTheme.Tint.error)
            .padding(.leading, 64 + AppTheme.Spacing.md)
            .padding(.bottom, AppTheme.Spacing.sm)
            .accessibilityHint("Tap the warning icon again to dismiss")
    }

    // MARK: - Actions

    private func handleRowTap() {
        // No-op when subscribing or already subscribed — those states
        // dim the row visually and the trailing control communicates
        // them. Otherwise the row body acts as the primary subscribe
        // target. If the row currently shows an error, this retries
        // the subscribe (the ⚠ chip toggles the caption instead).
        guard !isRowTapDisabled else { return }
        onSubscribe()
    }

    private var accessibilityLabel: String {
        if isAlreadySubscribed {
            return "Already subscribed to \(result.collectionName)"
        }
        if isSubscribing {
            return "Subscribing to \(result.collectionName)"
        }
        if let rowError {
            return "Subscribe to \(result.collectionName). Last attempt failed: \(rowError)"
        }
        return "Subscribe to \(result.collectionName)"
    }
}
