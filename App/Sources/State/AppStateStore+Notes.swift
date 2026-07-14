import Foundation

// MARK: - Notes

extension AppStateStore {

    /// User-authored note path. Defaults `author: .user` and fires a
    /// fire-and-forget kind-1 publish through `UserIdentityStore.shared`
    /// per the wiring contract in `identity-05-synthesis.md` §5.3.
    /// Existing call-sites (`AgentNotesView`, `FriendDetailView`) hit this
    /// signature unchanged.
    @discardableResult
    func addNote(text: String, kind: NoteKind = .free, target: Anchor? = nil) -> Note {
        return addNote(text: text, kind: kind, target: target, author: .user)
    }

    /// Author-aware overload. The agent-tool path passes `author: .agent`
    /// so the note is appended locally without going through publish.
    @discardableResult
    func addNote(text: String, kind: NoteKind = .free, target: Anchor? = nil, author: NoteAuthor) -> Note {
        let note = Note(text: text, kind: kind, target: target, author: author)
        state.notes.append(note)
        if author == .user {
            // For episode-anchored notes, forward the episode ID as the coord
            // so the published kind:1 event carries an ["a", episodeID] tag.
            let episodeCoord: String?
            if case .episode(let id, _) = target {
                episodeCoord = id.uuidString
            } else {
                episodeCoord = nil
            }
            // Fire-and-forget — relay outage must never block a local action.
            Task { try? await UserIdentityStore.shared.publishUserNote(note, episodeCoord: episodeCoord) }
        }
        return note
    }

    /// All non-deleted notes anchored to a specific episode, sorted by
    /// position ascending so the chapter rail can interleave them naturally.
    func notes(forEpisode episodeID: UUID) -> [Note] {
        state.notes
            .filter { note in
                guard !note.deleted,
                      case .episode(let id, _) = note.target else { return false }
                return id == episodeID
            }
            .sorted {
                guard case .episode(_, let a) = $0.target,
                      case .episode(_, let b) = $1.target else { return false }
                return a < b
            }
    }

    func deleteNote(_ id: UUID) {
        guard let idx = state.notes.firstIndex(where: { $0.id == id }) else { return }
        state.notes[idx].deleted = true
    }

    func restoreNote(_ id: UUID) {
        guard let idx = state.notes.firstIndex(where: { $0.id == id }) else { return }
        state.notes[idx].deleted = false
    }

    func updateNote(_ note: Note) {
        guard let idx = state.notes.firstIndex(where: { $0.id == note.id }) else { return }
        state.notes[idx] = note
    }

    func clearAllNotes() {
        var updated = state.notes
        for idx in updated.indices where !updated[idx].deleted {
            updated[idx].deleted = true
        }
        state.notes = updated
    }
}
