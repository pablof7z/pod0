import Pod0Core
import XCTest
@testable import Podcastr

final class SharedNoteMappingTests: XCTestCase {
    func testUnsupportedFutureNoteValuesFailClosedInNativeProjection() {
        XCTAssertNil(record(kind: .unsupported(wireCode: 41)).swiftValue)
        XCTAssertNil(record(author: .unsupported(wireCode: 42)).swiftValue)
    }

    /// An unknown target degrades the anchor; it must not remove the note.
    /// A build older than the version that wrote the target still shows the
    /// text. Dropping the record reads to the user as their note being deleted.
    func testUnsupportedTargetKeepsTheNoteAndDropsOnlyTheAnchor() {
        let note = record(target: .unsupported(wireCode: 43)).swiftValue
        XCTAssertNotNil(note)
        XCTAssertEqual(note?.text, "Future-safe note")
        XCTAssertNil(note?.target)
    }

    private func record(
        kind: Pod0Core.NoteKind = .free,
        author: Pod0Core.NoteAuthor = .user,
        target: Pod0Core.NoteTarget? = nil
    ) -> NoteRecord {
        NoteRecord(
            noteId: NoteId(high: 1, low: 2),
            revision: NoteRevision(value: 1),
            text: "Future-safe note",
            kind: kind,
            author: author,
            target: target,
            createdAt: UnixTimestampMilliseconds(value: 1_700_000_000_000),
            deleted: false,
            evidence: nil
        )
    }
}
