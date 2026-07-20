import CSQLiteVec
import Foundation

extension EpisodeSQLiteStore {
    func loadAll(loadLegacyChapterAdjuncts: Bool = true) throws -> [Episode] {
        let decoder = Self.decoder(
            loadLegacyChapterAdjuncts: loadLegacyChapterAdjuncts
        )
        return try withDatabase { db in
            try ensureSchema(in: db)
            let statement = try prepare(
                """
                SELECT payload
                FROM episodes
                ORDER BY sort_order ASC
                """,
                in: db
            )
            defer { sqlite3_finalize(statement) }

            var episodes: [Episode] = []
            while true {
                let code = sqlite3_step(statement)
                if code == SQLITE_DONE { break }
                guard code == SQLITE_ROW else {
                    throw EpisodeSQLiteStoreError.step(Self.errorMessage(db))
                }
                guard let bytes = sqlite3_column_blob(statement, 0) else {
                    throw EpisodeSQLiteStoreError.decode("missing episode payload")
                }
                let count = Int(sqlite3_column_bytes(statement, 0))
                let data = Data(bytes: bytes, count: count)
                do {
                    episodes.append(try decoder.decode(Episode.self, from: data))
                } catch {
                    throw EpisodeSQLiteStoreError.decode(error.localizedDescription)
                }
            }
            return episodes
        }
    }
}
