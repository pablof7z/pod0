import SwiftUI

// MARK: - ChapterRailItem

/// Discriminated union representing an item in the chapter rail — either a
/// publisher chapter or an episode-anchored note. Both carry a `sortTime`
/// so the rail can interleave them chronologically in a single sorted pass.
enum ChapterRailItem: Identifiable {
    case chapter(Episode.Chapter)
    case note(Note)

    var id: UUID {
        switch self {
        case .chapter(let c): return c.id
        case .note(let n):    return n.id
        }
    }

    /// The timeline position used to sort this item in the rail.
    var sortTime: TimeInterval {
        switch self {
        case .chapter(let c): return c.startTime
        case .note(let n):
            guard case .episode(_, let pos) = n.target else { return 0 }
            return pos
        }
    }
}

// MARK: - Note row for the chapter rail

extension PlayerChaptersScrollView {

    /// A compact note row rendered between chapter rows in the chapter rail.
    /// Visually lighter than a chapter row — secondary text, smaller icon —
    /// so the eye reads chapters as structure and notes as annotations.
    @ViewBuilder
    func noteRow(_ note: Note) -> some View {
        let positionSeconds: TimeInterval = {
            guard case .episode(_, let pos) = note.target else { return 0 }
            return pos
        }()

        HStack(alignment: .top, spacing: AppTheme.Spacing.sm) {
            Image(systemName: noteIcon(for: note))
                .font(.caption2.weight(.semibold))
                .foregroundStyle(noteColor(for: note))
                .frame(width: 14, alignment: .center)
                .padding(.top, 2)

            Text(note.text)
                .font(.system(.subheadline))
                .foregroundStyle(Color.secondary)
                .multilineTextAlignment(.leading)
                .lineLimit(4)

            Spacer(minLength: 0)

            Text(formatNoteTimestamp(positionSeconds))
                .font(.system(.caption, design: .monospaced).weight(.medium))
                .foregroundStyle(.tertiary)
        }
        .padding(.horizontal, AppTheme.Spacing.sm)
        .padding(.vertical, AppTheme.Spacing.xs)
        .contextMenu {
            Button(role: .destructive) {
                store.deleteNote(note.id)
            } label: {
                Label("Delete Note", systemImage: "trash")
            }
        }
    }

    // MARK: - Helpers

    private func noteIcon(for note: Note) -> String {
        switch note.kind {
        case .reflection: return "sparkles"
        case .free:       return "note.text"
        case .systemEvent: return "gear"
        }
    }

    private func noteColor(for note: Note) -> Color {
        switch note.kind {
        case .reflection: return .orange
        case .free:       return .indigo
        case .systemEvent: return .secondary
        }
    }

    /// Same zero-padded `mm:ss` / `h:mm:ss` format as `formatTimestamp` in the
    /// main file, kept as a distinct method so the two files stay independent.
    func formatNoteTimestamp(_ t: TimeInterval) -> String {
        guard t.isFinite, t >= 0 else { return "0:00" }
        let total = Int(t)
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        return h > 0
            ? String(format: "%02d:%02d:%02d", h, m, s)
            : String(format: "%02d:%02d", m, s)
    }
}
