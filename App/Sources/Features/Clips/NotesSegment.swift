import SwiftUI

// MARK: - NotesSegment

/// The "Notes" segment of `SavedView` — everything the reader has written,
/// newest first, with the passage it was written against.
///
/// A note anchored inside a clip's span opens that clip's transcript, because
/// the note only means something next to the thing it is about. A note with no
/// clip around it opens its episode instead.
struct NotesSegment: View {

    @Environment(AppStateStore.self) private var store

    let searchQuery: String
    let onOpenEpisode: (UUID) -> Void
    let onOpenClip: (UUID) -> Void

    var body: some View {
        let entries = filtered(allEntries())
        if entries.isEmpty {
            emptyState
        } else {
            List {
                ForEach(entries) { entry in
                    row(entry)
                        .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 6, trailing: 16))
                        .listRowBackground(Color.clear)
                        .listRowSeparator(.hidden)
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
        }
    }

    // MARK: - Row

    private func row(_ entry: NoteEntry) -> some View {
        Button {
            Haptics.selection()
            if let clipID = entry.clipID {
                onOpenClip(clipID)
            } else if let episodeID = entry.episodeID {
                onOpenEpisode(episodeID)
            }
        } label: {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
                Text(entry.note.text)
                    .font(.system(size: 16))
                    .lineSpacing(2)
                    .foregroundStyle(.primary)
                    .lineLimit(4)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)

                if let passage = entry.passage, !passage.isEmpty {
                    Text(passage)
                        .font(.system(size: 13))
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                        .padding(.leading, AppTheme.Spacing.sm)
                        .overlay(alignment: .leading) {
                            Capsule()
                                .fill(Color.accentColor.opacity(0.45))
                                .frame(width: 2)
                        }
                }

                HStack(spacing: AppTheme.Spacing.xs) {
                    if let show = entry.showTitle {
                        Text(show)
                            .font(.system(.caption2, design: .monospaced))
                            .tracking(0.7)
                            .textCase(.uppercase)
                            .lineLimit(1)
                    }
                    Spacer(minLength: 0)
                    Text(entry.note.createdAt, format: .relative(presentation: .named))
                        .font(.caption2)
                }
                .foregroundStyle(.tertiary)
            }
            .padding(AppTheme.Spacing.md)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                Color(.secondarySystemBackground),
                in: RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
            )
        }
        .buttonStyle(.pressable(scale: 0.98))
    }

    private var emptyState: some View {
        ContentUnavailableView {
            Label("Nothing Written Yet", systemImage: "text.quote")
        } description: {
            Text("Open a clip and pull up on the passage to write against it.")
        }
    }

    // MARK: - Entries

    private struct NoteEntry: Identifiable {
        let note: Note
        let clipID: UUID?
        let episodeID: UUID?
        let passage: String?
        let showTitle: String?
        var id: UUID { note.id }
    }

    /// Resolves each note to the passage it was written against.
    ///
    /// A clip-targeted note names its clip outright. An episode-anchored
    /// moment-note has no clip, but if one of the reader's clips happens to
    /// span its position we show that passage anyway — the note was written
    /// about that moment, and seeing it beside the words is the whole point.
    private func allEntries() -> [NoteEntry] {
        let clips = store.allClips()
        return store.state.notes
            .filter { !$0.deleted && $0.author == .user }
            .sorted { $0.createdAt > $1.createdAt }
            .map { note in entry(for: note, clips: clips) }
    }

    private func entry(for note: Note, clips: [Clip]) -> NoteEntry {
        switch note.target {
        case .clip(let clipID):
            let clip = clips.first { $0.id == clipID }
            return NoteEntry(
                note: note,
                clipID: clipID,
                episodeID: clip?.episodeID,
                passage: clip?.transcriptText,
                showTitle: showTitle(episodeID: clip?.episodeID)
            )
        case .episode(let episodeID, let position):
            let clip = clips.first {
                $0.episodeID == episodeID
                    && position >= $0.startSeconds
                    && position <= $0.endSeconds
            }
            return NoteEntry(
                note: note,
                clipID: clip?.id,
                episodeID: episodeID,
                passage: clip?.transcriptText,
                showTitle: showTitle(episodeID: episodeID)
            )
        case .note, .none:
            return NoteEntry(note: note, clipID: nil, episodeID: nil, passage: nil, showTitle: nil)
        }
    }

    private func showTitle(episodeID: UUID?) -> String? {
        guard let episodeID else { return nil }
        return store.episode(id: episodeID).flatMap { store.podcast(id: $0.podcastID)?.title }
    }

    private func filtered(_ entries: [NoteEntry]) -> [NoteEntry] {
        guard !searchQuery.isEmpty else { return entries }
        let query = searchQuery.lowercased()
        return entries.filter {
            $0.note.text.lowercased().contains(query)
                || ($0.passage?.lowercased().contains(query) ?? false)
                || ($0.showTitle?.lowercased().contains(query) ?? false)
        }
    }
}
