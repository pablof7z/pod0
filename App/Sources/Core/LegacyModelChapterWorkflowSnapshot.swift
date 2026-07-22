import Foundation
import Pod0Core

struct LegacyModelChapterWorkflowSnapshot: Equatable {
    let sourceGeneration: UInt64
    let candidates: [LegacyModelChapterCutoverCandidate]
    let backup: LegacyModelChapterWorkflowBackupManifest

    static func capture(from store: JobStore) throws -> Self {
        let jobs = try store.legacyChapterJobs(kind: .chapterArtifacts)
        let digest = try sourceFingerprint(for: jobs)
        let generation = sourceGeneration(for: digest)
        let classified = jobs.map { ($0, candidate($0)) }
        return Self(
            sourceGeneration: generation,
            candidates: classified.compactMap(\.1),
            backup: LegacyModelChapterWorkflowBackupManifest(
                sourceGeneration: generation,
                sourceFingerprint: digest,
                rows: classified.map {
                    LegacyModelChapterWorkflowBackupRow(
                        job: $0.0,
                        classification: .classify(job: $0.0, candidate: $0.1)
                    )
                }
            )
        )
    }

    static func candidate(
        _ job: LegacyChapterWorkflowJob
    ) -> LegacyModelChapterCutoverCandidate? {
        let disposition: LegacyModelChapterCutoverDisposition
        switch job.state {
        case .succeeded:
            disposition = parsedSuccessReceipt(job).map {
                .succeeded(
                    artifactId: $0.artifactID,
                    contentDigest: $0.contentDigest,
                    integrityDigest: $0.integrityDigest,
                    selectionRevision: StateRevision(value: $0.selectionRevision)
                )
            } ?? .ambiguous
        case .leased, .running, .retryScheduled:
            disposition = .ambiguous
        case .blocked:
            if job.lastErrorClass == .unsafeToRetry {
                disposition = .ambiguous
            } else {
                disposition = .blocked(
                    failureCode: failureCode(for: job.lastErrorClass),
                    failureDetail: boundedDetail(job.lastErrorMessage),
                    mayHaveSubmitted: mayHaveSubmitted(job)
                )
            }
        case .failedPermanent:
            disposition = .failed(
                failureCode: failureCode(for: job.lastErrorClass),
                failureDetail: boundedDetail(job.lastErrorMessage),
                mayHaveSubmitted: mayHaveSubmitted(job)
            )
        case .cancelled:
            disposition = .cancelled(mayHaveSubmitted: job.attempt > 0)
        case .pending:
            guard job.attempt > 0 else { return nil }
            disposition = .ambiguous
        case .obsolete:
            guard job.attempt > 0 else { return nil }
            disposition = .ambiguous
        }
        return LegacyModelChapterCutoverCandidate(
            episodeId: EpisodeId(uuid: job.subjectID),
            inputVersion: job.inputVersion,
            disposition: disposition
        )
    }

    /// Parses bounded legacy receipt evidence. Rust verifies that the referenced
    /// artifact and selection are authoritative before adopting it as success.
    private static func parsedSuccessReceipt(
        _ job: LegacyChapterWorkflowJob
    ) -> ParsedSuccess? {
        guard let encoded = job.outputVersion,
              let data = Data(base64Encoded: encoded),
              let receipt = try? JSONDecoder().decode(
                LegacySharedChapterWorkflowReceiptV1.self,
                from: data
              ),
              receipt.schemaVersion == LegacySharedChapterWorkflowReceiptV1.schemaVersion,
              receipt.episodeID == job.subjectID,
              receipt.inputVersion == job.inputVersion,
              let artifactUUID = UUID(uuidString: receipt.artifactID),
              let contentDigest = ContentDigest(hexadecimal: receipt.contentDigest),
              let integrityDigest = ContentDigest(hexadecimal: receipt.integrityDigest)
        else { return nil }
        return ParsedSuccess(
            artifactID: ChapterArtifactId(uuid: artifactUUID),
            contentDigest: contentDigest,
            integrityDigest: integrityDigest,
            selectionRevision: receipt.selectionRevision
        )
    }

    private static func mayHaveSubmitted(_ job: LegacyChapterWorkflowJob) -> Bool {
        guard job.attempt > 0 else { return false }
        switch job.lastErrorClass {
        case .missingCredential, .missingDependency, .unsupportedFormat, .invalidInput:
            return false
        default:
            return true
        }
    }

    private static func failureCode(for error: JobErrorClass?) -> String {
        switch error {
        case .missingCredential: "missing_credential"
        case .missingDependency: "storage_unavailable"
        case .rateLimited: "rate_limited"
        case .offline: "offline"
        case .network, .transient: "transport"
        case .unsupportedFormat, .invalidInput: "invalid_request"
        case .unsafeToRetry: "ambiguous_submission"
        case .corruptArtifact: "qualification_rejected"
        case .cancelled: "cancelled"
        case .unexpected, nil: "retry_exhausted"
        }
    }

    private static func boundedDetail(_ value: String?) -> String? {
        guard let value else { return nil }
        let bytes = Array(value.utf8.prefix(16_384))
        for count in stride(from: bytes.count, through: 0, by: -1) {
            if let result = String(bytes: bytes.prefix(count), encoding: .utf8) {
                return result
            }
        }
        return nil
    }

    static func sourceFingerprint(for jobs: [LegacyChapterWorkflowJob]) throws -> String {
        try LegacyChapterWorkflowSource.fingerprint(for: jobs)
    }

    static func sourceGeneration(for fingerprint: String) -> UInt64 {
        LegacyChapterWorkflowSource.generation(for: fingerprint)
    }

    private struct ParsedSuccess {
        let artifactID: ChapterArtifactId
        let contentDigest: ContentDigest
        let integrityDigest: ContentDigest
        let selectionRevision: UInt64
    }
}
