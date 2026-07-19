import Pod0Core
import XCTest
@testable import Podcastr

final class SharedNoteMappingTests: XCTestCase {
    func testUnsupportedFutureNoteValuesFailClosedInNativeProjection() {
        XCTAssertNil(record(kind: .unsupported(wireCode: 41)).swiftValue)
        XCTAssertNil(record(author: .unsupported(wireCode: 42)).swiftValue)
        XCTAssertNil(record(target: .unsupported(wireCode: 43)).swiftValue)
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
