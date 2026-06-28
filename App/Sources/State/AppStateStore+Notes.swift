import Foundation

// MARK: - Notes

extension AppStateStore {

    /// User-authored note path. Defaults `author: .user` and fires a
    /// fire-and-forget kind-1 publish through the store-owned `identity`
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
        dispatchAddNoteToKernel(note)
        // Bounded optimistic echo. The next kernel snapshot replaces
        // `state.notes` from `PodcastUpdate.notes`.
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
            Task { try? await identity.publishUserNote(note, episodeCoord: episodeCoord) }
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
        kernel?.dispatch(namespace: "podcast.social",
                         body: ["op": "delete_note", "id": id.uuidString])
        guard let idx = state.notes.firstIndex(where: { $0.id == id }) else { return }
        state.notes[idx].deleted = true
    }

    func restoreNote(_ id: UUID) {
        kernel?.dispatch(namespace: "podcast.social",
                         body: ["op": "restore_note", "id": id.uuidString])
        guard let idx = state.notes.firstIndex(where: { $0.id == id }) else { return }
        state.notes[idx].deleted = false
    }

    func updateNote(_ note: Note) {
        dispatchUpdateNoteToKernel(note)
        guard let idx = state.notes.firstIndex(where: { $0.id == note.id }) else { return }
        state.notes[idx] = note
    }

    func clearAllNotes() {
        kernel?.dispatch(namespace: "podcast.social",
                         body: ["op": "clear_notes"])
        var updated = state.notes
        for idx in updated.indices where !updated[idx].deleted {
            updated[idx].deleted = true
        }
        state.notes = updated
    }

    static func note(from summary: NoteSummary) -> Note? {
        guard let id = UUID(uuidString: summary.id) else { return nil }
        let kind = NoteKind(rawValue: summary.kind) ?? .free
        let author = NoteAuthor(rawValue: summary.author) ?? .user
        var note = Note(
            text: summary.text,
            kind: kind,
            target: anchor(from: summary.target),
            author: author
        )
        note.id = id
        note.createdAt = Date(timeIntervalSince1970: TimeInterval(summary.createdAt))
        note.deleted = summary.deleted
        return note
    }

    private func dispatchAddNoteToKernel(_ note: Note) {
        var body: [String: Any] = [
            "op": "add_note",
            "id": note.id.uuidString,
            "text": note.text,
            "kind": note.kind.rawValue,
            "created_at": Int(note.createdAt.timeIntervalSince1970),
            "author": note.author.rawValue
        ]
        if let target = Self.kernelTarget(from: note.target) {
            body["target"] = target
        }
        kernel?.dispatch(namespace: "podcast.social", body: body)
    }

    private func dispatchUpdateNoteToKernel(_ note: Note) {
        var body: [String: Any] = [
            "op": "update_note",
            "id": note.id.uuidString,
            "text": note.text,
            "kind": note.kind.rawValue
        ]
        if let target = Self.kernelTarget(from: note.target) {
            body["target"] = target
        }
        kernel?.dispatch(namespace: "podcast.social", body: body)
    }

    private static func kernelTarget(from anchor: Anchor?) -> [String: Any]? {
        guard let anchor else { return nil }
        switch anchor {
        case .note(let id):
            return ["type": "note", "note_id": id.uuidString]
        case .friend(let id):
            return ["type": "friend", "friend_id": id.uuidString]
        case .episode(let id, let positionSeconds):
            return [
                "type": "episode",
                "episode_id": id.uuidString,
                "position_secs": positionSeconds
            ]
        }
    }

    private static func anchor(from target: NoteTargetSummary?) -> Anchor? {
        guard let target else { return nil }
        switch target.type {
        case "note":
            return target.noteId
                .flatMap(UUID.init(uuidString:))
                .map { .note(id: $0) }
        case "friend":
            return target.friendId
                .flatMap(UUID.init(uuidString:))
                .map { .friend(id: $0) }
        case "episode":
            guard let id = target.episodeId.flatMap(UUID.init(uuidString:)) else { return nil }
            return .episode(id: id, positionSeconds: target.positionSecs ?? 0)
        default:
            return nil
        }
    }
}
