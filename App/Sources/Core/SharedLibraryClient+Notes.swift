import Foundation
import Pod0Core

struct SharedNoteSnapshot: @unchecked Sendable {
    let collectionRevision: StateRevision
    let notes: [Note]
    let operations: [OperationProjection]
}

extension SharedLibraryClient {
    func receiveNotes(revision: UInt64) {
        guard revision >= lastNotesRevision else { return }
        lastNotesRevision = revision
        let facade = facade
        noteProjectionTask?.cancel()
        noteProjectionTask = Task { @MainActor [weak self] in
            let snapshot = await Task.detached(priority: .utility) {
                Self.loadNotePages(facade: facade, scope: .all)
            }.value
            guard !Task.isCancelled, let self, revision == lastNotesRevision else { return }
            cachedNotes = snapshot
            store?.applySharedNotes(snapshot)
            resolveWaiters(snapshot.operations)
        }
    }

    func notes(forEpisode episodeID: UUID) -> [Note] {
        cachedNotes?.notes.filter {
            guard !$0.deleted, case .episode(let id, _) = $0.target else { return false }
            return id == episodeID
        }.sorted {
            guard case .episode(_, let lhsPosition) = $0.target,
                  case .episode(_, let rhsPosition) = $1.target
            else { return $0.createdAt < $1.createdAt }
            return lhsPosition < rhsPosition
        } ?? []
    }

    func createNote(
        text: String,
        kind: NoteKind,
        target: Anchor?,
        author: NoteAuthor
    ) async throws -> Note {
        let result = try await execute(.createNote(
            text: text,
            kind: kind.coreValue,
            author: author.coreValue,
            target: try target?.coreValue()
        ))
        let snapshot = await refreshNoteSnapshot()
        guard case .noteCreated(let noteID) = result,
              let id = noteID.uuid,
              let note = snapshot.notes.first(where: { $0.id == id })
        else { throw SharedLibraryError.unavailable }
        return note
    }

    func updateNote(_ note: Note) async throws {
        _ = try await execute(.updateNote(
            noteId: NoteId(uuid: note.id),
            expectedNoteRevision: NoteRevision(value: note.revision),
            text: note.text,
            kind: note.kind.coreValue,
            target: try note.target?.coreValue()
        ))
        _ = await refreshNoteSnapshot()
    }

    func setNoteDeleted(_ note: Note, deleted: Bool) async throws {
        _ = try await execute(.setNoteDeleted(
            noteId: NoteId(uuid: note.id),
            expectedNoteRevision: NoteRevision(value: note.revision),
            deleted: deleted
        ))
        _ = await refreshNoteSnapshot()
    }

    func clearNotes() async throws {
        let revision = try await noteCollectionRevision()
        _ = try await execute(.clearNotes(expectedCollectionRevision: revision))
        _ = await refreshNoteSnapshot()
    }

    nonisolated static func loadNotePages(
        facade: Pod0Facade,
        scope: NoteProjectionScope
    ) -> SharedNoteSnapshot {
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

    private func noteCollectionRevision() async throws -> StateRevision {
        if let revision = cachedNotes?.collectionRevision { return revision }
        let facade = facade
        let snapshot = await Task.detached(priority: .utility) {
            Self.loadNotePages(facade: facade, scope: .all)
        }.value
        cachedNotes = snapshot
        store?.applySharedNotes(snapshot)
        return snapshot.collectionRevision
    }

    private func refreshNoteSnapshot() async -> SharedNoteSnapshot {
        let facade = facade
        let snapshot = await Task.detached(priority: .utility) {
            Self.loadNotePages(facade: facade, scope: .all)
        }.value
        cachedNotes = snapshot
        store?.applySharedNotes(snapshot)
        return snapshot
    }
}
