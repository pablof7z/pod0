import Foundation
import Pod0Core

struct SharedNoteSnapshot {
    let collectionRevision: StateRevision
    let notes: [Note]
    let operations: [OperationProjection]
}

extension SharedLibraryClient {
    func receiveNotes(revision: UInt64) {
        guard revision >= lastNotesRevision else { return }
        lastNotesRevision = revision
        let snapshot = loadNotePages(scope: .all)
        cachedNotes = snapshot
        store?.applySharedNotes(snapshot)
        resolveWaiters(snapshot.operations)
    }

    func notes(forEpisode episodeID: UUID) -> [Note] {
        loadNotePages(scope: .episode(episodeId: EpisodeId(uuid: episodeID))).notes
    }

    func createNote(
        text: String,
        kind: NoteKind,
        target: Anchor?,
        author: NoteAuthor
    ) throws -> Note {
        let result = try executeNoteCommand(.createNote(
            text: text,
            kind: kind.coreValue,
            author: author.coreValue,
            target: try target?.coreValue()
        ))
        guard case .noteCreated(let noteID) = result,
              let id = noteID.uuid,
              let note = cachedNotes?.notes.first(where: { $0.id == id })
        else { throw SharedLibraryError.unavailable }
        return note
    }

    func updateNote(_ note: Note) throws {
        _ = try executeNoteCommand(.updateNote(
            noteId: NoteId(uuid: note.id),
            expectedNoteRevision: NoteRevision(value: note.revision),
            text: note.text,
            kind: note.kind.coreValue,
            target: try note.target?.coreValue()
        ))
    }

    func setNoteDeleted(_ note: Note, deleted: Bool) throws {
        _ = try executeNoteCommand(.setNoteDeleted(
            noteId: NoteId(uuid: note.id),
            expectedNoteRevision: NoteRevision(value: note.revision),
            deleted: deleted
        ))
    }

    func clearNotes() throws {
        let revision = cachedNotes?.collectionRevision ?? loadNotePages(scope: .all).collectionRevision
        _ = try executeNoteCommand(.clearNotes(expectedCollectionRevision: revision))
    }

    func loadNotePages(scope: NoteProjectionScope) -> SharedNoteSnapshot {
        var offset: UInt32 = 0
        var collectionRevision = StateRevision(value: 1)
        var notes: [Note] = []
        var operations: [OperationProjection] = []
        while true {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .notes(scope: scope),
                offset: offset,
                maxItems: 200
            ))
            guard case .notes(let page) = envelope.projection else { break }
            collectionRevision = page.collectionRevision
            notes.append(contentsOf: page.notes.compactMap(\.swiftValue))
            if operations.isEmpty { operations = page.operations }
            guard page.hasMore, offset <= UInt32.max - 200 else { break }
            offset += 200
        }
        return SharedNoteSnapshot(
            collectionRevision: collectionRevision,
            notes: notes,
            operations: operations
        )
    }

    private func executeNoteCommand(_ command: ApplicationCommand) throws -> OperationResult? {
        let commandID = CommandId(uuid: UUID())
        facade.dispatch(command: CommandEnvelope(
            commandId: commandID,
            cancellationId: CancellationId(uuid: UUID()),
            expectedRevision: nil,
            command: command
        ))
        let snapshot = loadNotePages(scope: .all)
        cachedNotes = snapshot
        store?.applySharedNotes(snapshot)
        guard let operation = snapshot.operations.first(where: { $0.commandId == commandID })
        else { throw SharedLibraryError.unavailable }
        switch operation.stage {
        case .succeeded:
            return operation.result
        case .failed, .cancelled, .unsupported:
            throw SharedLibraryError(operation.failure?.code)
        case .accepted, .running, .blocked:
            throw SharedLibraryError.unavailable
        }
    }
}
