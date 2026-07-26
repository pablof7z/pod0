import Foundation
import Pod0Core

enum SharedNoteMappingError: Error {
    case invalidAnchor
}

extension NoteRecord {
    var swiftValue: Note? {
        guard let id = noteId.uuid,
              let kind = kind.swiftValue,
              let author = author.swiftValue
        else { return nil }
        // Degrade, never drop. A target this build cannot name — one written by
        // a newer version — costs the note its anchor, not its existence. The
        // previous `return nil` removed the whole record from the projection,
        // which the user reads as their note being deleted. Losing where a note
        // pointed is recoverable; losing what it said is not.
        //
        // `kind` and `author` above still drop the record on an unknown value.
        // Same hazard, but neither is Optional, so degrading them means picking
        // a default that lies about the stored value — left alone deliberately.
        let target = self.target.flatMap(\.swiftValue)
        return Note(
            id: id,
            revision: revision.value,
            text: text,
            kind: kind,
            target: target,
            createdAt: createdAt.date,
            deleted: deleted,
            author: author,
            evidence: evidence?.swiftValue
        )
    }
}

extension NoteKind {
    var coreValue: Pod0Core.NoteKind {
        switch self {
        case .free: .free
        case .reflection: .reflection
        case .systemEvent: .systemEvent
        }
    }
}

extension NoteAuthor {
    var coreValue: Pod0Core.NoteAuthor {
        switch self {
        case .user: .user
        case .agent: .agent
        }
    }
}

extension Anchor {
    func coreValue() throws -> Pod0Core.NoteTarget {
        switch self {
        case .note(let id):
            return .note(noteId: NoteId(uuid: id))
        case .episode(let id, let positionSeconds):
            let milliseconds = positionSeconds * 1_000
            guard milliseconds.isFinite,
                  milliseconds >= 0,
                  milliseconds <= Double(UInt64.max)
            else { throw SharedNoteMappingError.invalidAnchor }
            return .episode(
                episodeId: EpisodeId(uuid: id),
                positionMilliseconds: UInt64(milliseconds.rounded())
            )
        }
    }
}

private extension Pod0Core.NoteKind {
    var swiftValue: NoteKind? {
        switch self {
        case .free: .free
        case .reflection: .reflection
        case .systemEvent: .systemEvent
        case .unsupported: nil
        }
    }
}

private extension Pod0Core.NoteAuthor {
    var swiftValue: NoteAuthor? {
        switch self {
        case .user: .user
        case .agent: .agent
        case .unsupported: nil
        }
    }
}

private extension Pod0Core.NoteTarget {
    var swiftValue: Anchor? {
        switch self {
        case .note(let noteID):
            noteID.uuid.map(Anchor.note(id:))
        case .episode(let episodeID, let positionMilliseconds):
            episodeID.uuid.map {
                Anchor.episode(
                    id: $0,
                    positionSeconds: Double(positionMilliseconds) / 1_000
                )
            }
        case .unsupported:
            nil
        }
    }
}

private extension NoteEvidenceReference {
    var swiftValue: NoteEvidence {
        NoteEvidence(
            generationID: generationId.stableString,
            transcriptVersionID: transcriptVersionId.stableString,
            transcriptContentDigest: transcriptContentDigest.stableString,
            spanID: spanId.stableString
        )
    }
}
