import CSQLiteVec
import Foundation
import XCTest
@testable import Podcastr

@MainActor
enum SharedChapterRecoveryTestSupport {
    enum FixtureError: Error {
        case missingEpisodePayload
    }

    static let chapterID = UUID(uuidString: "55555555-5555-5555-5555-555555555555")!

    static func injectLegacyChapters(
        _ fixture: SharedTranscriptRecoveryTestSupport.Fixture
    ) throws {
        try WorkflowSQLite.withDatabase(fileURL: fixture.persistence.episodeStore.fileURL) { db in
            let read = try WorkflowSQLite.prepare(
                "SELECT payload FROM episodes WHERE id=?",
                db: db
            )
            defer { sqlite3_finalize(read) }
            try WorkflowSQLite.bind(fixture.episodeID.uuidString, 1, read, db)
            guard sqlite3_step(read) == SQLITE_ROW,
                  let payload = WorkflowSQLite.data(read, 0),
                  var object = try JSONSerialization.jsonObject(with: payload) as? [String: Any]
            else { throw FixtureError.missingEpisodePayload }
            object["chapters"] = [[
                "id": chapterID.uuidString,
                "startTime": 0.0,
                "endTime": 60.0,
                "title": "Recovered chapter",
                "includeInTableOfContents": true,
                "isAIGenerated": false,
                "summary": "Preserved summary"
            ]]
            object["adSegments"] = []
            let updated = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
            let write = try WorkflowSQLite.prepare(
                "UPDATE episodes SET payload=? WHERE id=?",
                db: db
            )
            defer { sqlite3_finalize(write) }
            try WorkflowSQLite.bind(updated, 1, write, db)
            try WorkflowSQLite.bind(fixture.episodeID.uuidString, 2, write, db)
            try WorkflowSQLite.stepDone(write, db)
        }
    }
}
