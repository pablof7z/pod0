import CSQLiteVec
import Foundation

extension JobStore {
    func projections(for query: WorkflowProjectionQuery) throws -> [WorkflowJobProjection] {
        let subjectIDs = Array(Set(query.subjectIDs)).sorted { $0.uuidString < $1.uuidString }
        let kinds = Array(Set(query.kinds)).sorted { $0.rawValue < $1.rawValue }
        let attentionKinds = Array(Set(query.attentionKinds)).sorted { $0.rawValue < $1.rawValue }
        guard (!subjectIDs.isEmpty && !kinds.isEmpty) || !attentionKinds.isEmpty else { return [] }

        return try withDatabase(publishChanges: false) { db in
            var clauses: [String] = []
            if !subjectIDs.isEmpty, !kinds.isEmpty {
                clauses.append(
                    "(subject_id IN (\(placeholders(subjectIDs.count))) "
                        + "AND kind IN (\(placeholders(kinds.count))))"
                )
            }
            if !attentionKinds.isEmpty {
                clauses.append(
                    "(kind IN (\(placeholders(attentionKinds.count))) "
                        + "AND state IN ('pending','leased','running','retryScheduled','blocked','failedPermanent'))"
                )
            }
            let statement = try WorkflowSQLite.prepare(
                """
                SELECT \(Self.columns) FROM jobs
                WHERE \(clauses.joined(separator: " OR "))
                ORDER BY updated_at DESC, id DESC
                LIMIT ?
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            var index: Int32 = 1
            if !subjectIDs.isEmpty, !kinds.isEmpty {
                for id in subjectIDs {
                    try WorkflowSQLite.bind(id.uuidString, index, statement, db)
                    index += 1
                }
                for kind in kinds {
                    try WorkflowSQLite.bind(kind.rawValue, index, statement, db)
                    index += 1
                }
            }
            for kind in attentionKinds {
                try WorkflowSQLite.bind(kind.rawValue, index, statement, db)
                index += 1
            }
            try WorkflowSQLite.bind(Int64(min(max(query.limit, 1), 1_000)), index, statement, db)

            var latest: [WorkflowJobKey: WorkflowJobProjection] = [:]
            for job in try readRows(statement) {
                let projection = WorkflowJobProjection(job: job)
                if latest[projection.key] == nil { latest[projection.key] = projection }
            }
            return latest.values.sorted {
                if $0.updatedAt != $1.updatedAt { return $0.updatedAt > $1.updatedAt }
                return $0.id.uuidString > $1.id.uuidString
            }
        }
    }

    private func placeholders(_ count: Int) -> String {
        Array(repeating: "?", count: count).joined(separator: ",")
    }
}
