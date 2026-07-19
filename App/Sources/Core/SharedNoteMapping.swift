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
        let target: Anchor?
        switch self.target {
        case nil:
            target = nil
        case .some(let value):
            guard let mapped = value.swiftValue else { return nil }
            target = mapped
        }
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
