import SwiftUI

// MARK: - ClipNoteSheet

/// The note, raised out of the passage.
///
/// Deliberately unlabelled. The sheet came up from the clip and the clip is
/// still on screen above it, so what this is for is already answered — a
/// header saying "Notes" would only be the design apologising for itself.
struct ClipNoteSheet: View {

    let clip: Clip
    /// The margin: notes targeting this clip, oldest first so the stack reads
    /// as strata. Moment-notes are a different register and do not appear here.
    let notes: [Note]
    let episode: Episode?
    let podcast: Podcast?
    let onPlay: () -> Void

    @Environment(AppStateStore.self) private var store

    @State private var isSharing = false
    @State private var draft = ""
    @State private var isSaving = false
    @FocusState private var isWriting: Bool

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                ForEach(notes) { note in
                    entry(note)
                }
                composer
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

    /// Open paper. No placeholder, no prompt, no "Add note" — the sheet came up
    /// out of the passage, so a bare caret already says everything a sentence
    /// of helper text would, and says it without making the margin a chore.
    private var composer: some View {
        HStack(alignment: .bottom, spacing: AppTheme.Spacing.sm) {
            TextField("", text: $draft, axis: .vertical)
                .font(.system(size: 15.5))
                .lineSpacing(2)
                .textInputAutocapitalization(.sentences)
                .focused($isWriting)
                .disabled(isSaving)
                .accessibilityLabel("Write about this clip")
            if !trimmedDraft.isEmpty {
                Button {
                    Task { await save() }
                } label: {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.system(size: 26))
                        .symbolRenderingMode(.hierarchical)
                }
                .buttonStyle(.plain)
                .disabled(isSaving)
                .accessibilityLabel("Save note")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var trimmedDraft: String {
        draft.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func save() async {
        let text = trimmedDraft
        guard !text.isEmpty, !isSaving else { return }
        isSaving = true
        defer { isSaving = false }
        let saved = await store.addNote(text: text, target: .clip(id: clip.id))
        if saved != nil {
            Haptics.selection()
            draft = ""
            isWriting = false
        }
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
