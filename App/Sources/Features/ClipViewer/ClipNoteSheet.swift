import SwiftUI

// MARK: - ClipNoteSheet

/// The note, raised out of the passage.
///
/// Deliberately unlabelled. The sheet came up from the clip and the clip is
/// still on screen above it, so what this is for is already answered — a
/// header saying "Notes" would only be the design apologising for itself.
struct ClipNoteSheet: View {

    let clip: Clip
    /// Notes already anchored inside this clip's span. Until the kernel can
    /// target a clip these are moment-notes the reader wrote while listening,
    /// which is the two-register display working as intended.
    let notes: [Note]
    let episode: Episode?
    let podcast: Podcast?
    let onPlay: () -> Void

    @State private var isSharing = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                ForEach(notes) { note in
                    entry(note)
                }
                if notes.isEmpty {
                    unwritten
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, AppTheme.Spacing.lg)
            .padding(.top, AppTheme.Spacing.lg)
            .padding(.bottom, AppTheme.Spacing.xl)
        }
        .scrollBounceBehavior(.basedOnSize)
        .safeAreaInset(edge: .bottom) { tools }
        .presentationDetents([.fraction(0.42), .large])
        .presentationDragIndicator(.visible)
        .presentationBackgroundInteraction(.enabled(upThrough: .fraction(0.42)))
        .presentationCornerRadius(AppTheme.Corner.xl)
        .sheet(isPresented: $isSharing) {
            if let episode, let podcast {
                ClipShareSheet(clip: clip, episode: episode, podcast: podcast)
            }
        }
    }

    // MARK: - Entries

    private func entry(_ note: Note) -> some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            Text(Self.stamp.string(from: note.createdAt))
                .font(.system(.caption2, design: .monospaced))
                .tracking(0.9)
                .textCase(.uppercase)
                .foregroundStyle(.primary.opacity(note.author == .agent ? 0.32 : 0.38))
            Text(note.text)
                .font(.system(size: 15.5))
                .lineSpacing(2)
                // Agent replies sit lighter — visible, never mistakable for yours.
                .foregroundStyle(.primary.opacity(note.author == .agent ? 0.62 : 0.94))
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Nothing written here yet. No count, no prompt, no empty-state art — an
    /// unannotated clip is a complete object, and the moment the page implies
    /// otherwise the margin becomes a queue.
    private var unwritten: some View {
        Rectangle()
            .fill(.clear)
            .frame(height: 44)
            .accessibilityHidden(true)
    }

    // MARK: - Tools

    private var tools: some View {
        GlassEffectContainer(spacing: 18) {
            HStack(spacing: AppTheme.Spacing.lg) {
                tool("square.and.arrow.up", "Share") { isSharing = true }
                    .disabled(episode == nil || podcast == nil)
                tool("play.circle", "Play in player", action: onPlay)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, AppTheme.Spacing.lg)
            .padding(.vertical, AppTheme.Spacing.md)
        }
        .background(.bar)
    }

    private func tool(_ symbol: String, _ label: String, action: @escaping () -> Void) -> some View {
        Button {
            Haptics.selection()
            action()
        } label: {
            Image(systemName: symbol)
                .font(.system(size: 19, weight: .regular))
                .frame(width: 30, height: 30)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(.primary.opacity(0.7))
        .accessibilityLabel(label)
    }

    private static let stamp: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "d MMMM"
        return formatter
    }()
}
