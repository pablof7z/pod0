import CSQLite3
import Foundation

extension JobStore {
    func projections(for query: WorkflowProjectionQuery) throws -> [WorkflowJobProjection] {
        let subjectIDs = Array(Set(query.subjectIDs)).sorted { $0.uuidString < $1.uuidString }
        let kinds = Array(Set(query.kinds)).sorted { $0.rawValue < $1.rawValue }
        let attentionKinds = Array(Set(query.attentionKinds)).sorted { $0.rawValue < $1.rawValue }
        let recentKinds = Array(Set(query.recentKinds)).sorted { $0.rawValue < $1.rawValue }
        guard (!subjectIDs.isEmpty && !kinds.isEmpty)
                || !attentionKinds.isEmpty || !recentKinds.isEmpty else { return [] }

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
            if !recentKinds.isEmpty {
                clauses.append("(kind IN (\(placeholders(recentKinds.count))))")
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
            for kind in recentKinds {
                try WorkflowSQLite.bind(kind.rawValue, index, statement, db)
                index += 1
            }
            try WorkflowSQLite.bind(Int64(min(max(query.limit, 1), 1_000)), index, statement, db)

            let historyKinds = Set(recentKinds)
            var selected: [WorkflowJobProjection] = []
            var latestKeys: Set<WorkflowJobKey> = []
            for job in try readRows(statement) {
                let projection = WorkflowJobProjection(job: job)
                if historyKinds.contains(job.kind) {
                    selected.append(projection)
                } else if latestKeys.insert(projection.key).inserted {
                    selected.append(projection)
                }
            }
            return selected.sorted {
                if $0.updatedAt != $1.updatedAt { return $0.updatedAt > $1.updatedAt }
                return $0.id.uuidString > $1.id.uuidString
            }
        }
    }

    private func placeholders(_ count: Int) -> String {
        Array(repeating: "?", count: count).joined(separator: ",")
    }
}
