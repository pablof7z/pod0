import Foundation

// MARK: - Notes

extension AppStateStore {

    /// User-authored note path. Defaults `author: .user`.
    /// Existing call-sites (`AgentNotesView`, `FriendDetailView`) hit this
    /// signature unchanged.
    @discardableResult
    func addNote(text: String, kind: NoteKind = .free, target: Anchor? = nil) -> Note? {
        addNote(text: text, kind: kind, target: target, author: .user)
    }

    /// Author-aware overload. The agent-tool path passes `author: .agent`
    /// so the shared core records the correct author without going through publish.
    @discardableResult
    func addNote(
        text: String,
        kind: NoteKind = .free,
        target: Anchor? = nil,
        author: NoteAuthor
    ) -> Note? {
        do {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            let note = try sharedLibrary.createNote(
                text: text,
                kind: kind,
                target: target,
                author: author
            )
            if author == .user {
                recordProductSignal(.once(
                    name: .noteCreated,
                    subjectID: note.id,
                    outcome: .created
                ))
            }
            return note
        } catch {
            Self.logger.error("Shared note creation failed: \(error.localizedDescription, privacy: .public)")
            return nil
        }
    }

    /// All non-deleted notes anchored to a specific episode, sorted by
    /// position ascending so the chapter rail can interleave them naturally.
    func notes(forEpisode episodeID: UUID) -> [Note] {
        sharedLibrary?.notes(forEpisode: episodeID) ?? []
    }

    @discardableResult
    func deleteNote(_ id: UUID) -> Bool {
        setNoteDeleted(id, deleted: true)
    }

    @discardableResult
    func restoreNote(_ id: UUID) -> Bool {
        setNoteDeleted(id, deleted: false)
    }

    @discardableResult
    func updateNote(_ note: Note) -> Bool {
        do {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            try sharedLibrary.updateNote(note)
            return true
        } catch {
            Self.logger.error("Shared note update failed: \(error.localizedDescription, privacy: .public)")
            return false
        }
    }

    @discardableResult
    func clearAllNotes() -> Bool {
        do {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            try sharedLibrary.clearNotes()
            return true
        } catch {
            Self.logger.error("Shared note clear failed: \(error.localizedDescription, privacy: .public)")
            return false
        }
    }

    private func setNoteDeleted(_ id: UUID, deleted: Bool) -> Bool {
        do {
            guard let note = state.notes.first(where: { $0.id == id }),
                  let sharedLibrary
            else { throw SharedLibraryError.notFound }
            try sharedLibrary.setNoteDeleted(note, deleted: deleted)
            return true
        } catch {
            Self.logger.error("Shared note deletion change failed: \(error.localizedDescription, privacy: .public)")
            return false
        }
    }
}
