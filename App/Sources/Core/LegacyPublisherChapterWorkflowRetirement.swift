import Foundation

enum LegacyPublisherChapterWorkflowBackupClassification: String, Codable, Sendable {
    case completedEvidence
    case safeIdempotentRederivation
    case cancelledOrObsoleteHistory
    case corruptUnsupportedEvidence

    static func classify(
        _ job: LegacyChapterWorkflowJob
    ) -> LegacyPublisherChapterWorkflowBackupClassification {
        guard job.payloadVersion == 1,
              let payload = job.payload,
              let decoded = try? JSONDecoder().decode(
                LegacyPublisherChaptersJobPayloadV1.self,
                from: payload
              ),
              decoded.sourceVersion == job.inputVersion,
              ["http", "https"].contains(decoded.url.scheme?.lowercased() ?? "")
        else { return .corruptUnsupportedEvidence }
        switch job.state {
        case .succeeded:
            return .completedEvidence
        case .cancelled, .obsolete:
            return .cancelledOrObsoleteHistory
        case .pending, .leased, .running, .retryScheduled, .blocked, .failedPermanent:
            // The shared-core replacement performs an idempotent HTTP GET from
            // current authoritative feed metadata; no legacy provider operation
            // is resumed or resubmitted from this row.
            return .safeIdempotentRederivation
        }
    }
}

struct LegacyPublisherChapterWorkflowBackupRow: Codable, Sendable, Equatable {
    let job: LegacyChapterWorkflowJob
    let classification: LegacyPublisherChapterWorkflowBackupClassification
}

struct LegacyPublisherChapterWorkflowBackupManifest: Codable, Sendable, Equatable {
    static let currentSchemaVersion = 1

    let schemaVersion: Int
    let sourceGeneration: UInt64
    let sourceFingerprint: String
    let rows: [LegacyPublisherChapterWorkflowBackupRow]

    init(jobs: [LegacyChapterWorkflowJob]) throws {
        let sorted = jobs.sorted { $0.id.uuidString < $1.id.uuidString }
        let fingerprint = try LegacyChapterWorkflowSource.fingerprint(for: sorted)
        schemaVersion = Self.currentSchemaVersion
        sourceGeneration = LegacyChapterWorkflowSource.generation(for: fingerprint)
        sourceFingerprint = fingerprint
        rows = sorted.map {
            LegacyPublisherChapterWorkflowBackupRow(
                job: $0,
                classification: .classify($0)
            )
        }
    }

    func publish(to root: URL) throws {
        try LegacyWorkflowBackupStorage.publish(
            self,
            to: root,
            destinationName: fileName,
            matchingPrefix: Self.prefix(sourceGeneration),
            temporaryPrefix: "publisher-chapter-workflows",
            validate: { try $0.validate() }
        )
    }

    static func load(
        from root: URL,
        sourceGeneration: UInt64,
        required: Bool = true
    ) throws -> Self? {
        let manifest: Self? = try LegacyWorkflowBackupStorage.load(
            from: root,
            matchingPrefix: prefix(sourceGeneration),
            required: required,
            validate: { try $0.validate() }
        )
        guard manifest?.sourceGeneration == sourceGeneration || manifest == nil else {
            throw LegacyChapterWorkflowBackupError.invalidBackup
        }
        return manifest
    }

    func matches(_ jobs: [LegacyChapterWorkflowJob]) -> Bool {
        rows.map(\.job) == jobs.sorted { $0.id.uuidString < $1.id.uuidString }
    }

    func validate() throws {
        let jobs = rows.map(\.job)
        let ids = Set(jobs.map(\.id))
        let fingerprint = try LegacyChapterWorkflowSource.fingerprint(for: jobs)
        guard schemaVersion == Self.currentSchemaVersion,
              sourceGeneration > 0,
              sourceFingerprint.count == 64,
              ids.count == rows.count,
              jobs.allSatisfy({ $0.kind == .publisherChapters }),
              fingerprint == sourceFingerprint,
              LegacyChapterWorkflowSource.generation(for: sourceFingerprint)
                == sourceGeneration,
              rows.allSatisfy({ $0.classification == .classify($0.job) })
        else { throw LegacyChapterWorkflowBackupError.invalidBackup }
    }

    private var fileName: String {
        "publisher-chapter-workflows-v1-\(sourceGeneration)-\(sourceFingerprint).json"
    }

    private static func prefix(_ sourceGeneration: UInt64) -> String {
        "publisher-chapter-workflows-v1-\(sourceGeneration)-"
    }
}

enum LegacyPublisherChapterWorkflowRetirement {
    static func run(
        jobStore: JobStore,
        backupRoot: URL,
        modelSourceGeneration: UInt64,
        now: Date = Date()
    ) throws {
        if let marker = try jobStore.legacyChapterWorkflowRetirementMarker() {
            guard marker.modelSourceGeneration == modelSourceGeneration,
                  let backup = try LegacyPublisherChapterWorkflowBackupManifest.load(
                    from: backupRoot,
                    sourceGeneration: marker.publisherSourceGeneration
                  ),
                  backup.sourceFingerprint == marker.publisherSourceFingerprint,
                  try jobStore.verifyLegacyChapterWorkflowRetirement(marker)
            else { throw LegacyChapterWorkflowBackupError.invalidBackup }
            return
        }

        let jobs = try jobStore.legacyChapterJobs(kind: .publisherChapters)
        let backup = try LegacyPublisherChapterWorkflowBackupManifest(jobs: jobs)
        try backup.publish(to: backupRoot)
        guard try LegacyPublisherChapterWorkflowBackupManifest.load(
            from: backupRoot,
            sourceGeneration: backup.sourceGeneration
        ) == backup else { throw LegacyChapterWorkflowBackupError.invalidBackup }

        let marker = LegacyChapterWorkflowRetirementMarker(
            modelSourceGeneration: modelSourceGeneration,
            publisherSourceGeneration: backup.sourceGeneration,
            publisherSourceFingerprint: backup.sourceFingerprint,
            completedAt: now
        )
        guard try jobStore.commitLegacyChapterWorkflowRetirement(
            expectedPublisherJobs: jobs,
            marker: marker
        ) else { throw LegacyChapterWorkflowBackupError.sourceChanged }
        guard try jobStore.verifyLegacyChapterWorkflowRetirement(marker) else {
            throw LegacyChapterWorkflowBackupError.invalidBackup
        }
    }
}

private struct LegacyPublisherChaptersJobPayloadV1: Codable {
    let url: URL
    let sourceVersion: String
}
