#if DEBUG
import Foundation

/// Simulator-only acceptance hook used by the process reconstruction script.
/// The seed process is terminated externally after acquiring a lease; a new
/// app process must recover solely from the SQLite file and finish the job.
@MainActor
enum WorkflowProcessReconstructionHarness {
    private struct Marker: Codable {
        let phase: String
        let jobID: UUID
        let firstLeaseToken: UUID
        let recoveredLeaseToken: UUID?
        let attempt: Int
        let state: WorkJobState
    }

    static func runIfRequested() -> Bool {
        guard let phase = ProcessInfo.processInfo.environment[
            "POD0_WORKFLOW_HARNESS_PHASE"
        ] else { return false }
        do {
            switch phase {
            case "seed": try seed()
            case "recover": try recover()
            default: throw JobStoreError.corruptRow
            }
        } catch {
            try? writeFailure(phase: phase, error: error)
        }
        return true
    }

    private static func seed() throws {
        try? FileManager.default.removeItem(at: rootURL)
        try FileManager.default.createDirectory(at: rootURL, withIntermediateDirectories: true)
        let store = JobStore(fileURL: databaseURL)
        let subject = UUID()
        _ = try store.ensureJob(DesiredJob(
            idempotencyKey: "process-reconstruction:v1",
            kind: .metadataIndex,
            subjectID: subject,
            inputVersion: "v1",
            resourceClass: .embedding
        ), notBefore: Date(timeIntervalSince1970: 1_000))
        let job = try store.claimDueJobs(
            resourceClass: .embedding,
            capacity: 1,
            now: Date(timeIntervalSince1970: 1_000),
            owner: "seed-process",
            leaseDuration: 1
        ).first ?? { throw JobStoreError.corruptRow }()
        let token = try job.leaseToken ?? { throw JobStoreError.corruptRow }()
        try store.markRunning(
            id: job.id,
            leaseToken: token,
            now: Date(timeIntervalSince1970: 1_000)
        )
        try write(Marker(
            phase: "seed",
            jobID: job.id,
            firstLeaseToken: token,
            recoveredLeaseToken: nil,
            attempt: job.attempt,
            state: .running
        ), name: "seed.json")
    }

    private static func recover() throws {
        let store = JobStore(fileURL: databaseURL)
        let abandoned = try store.job(idempotencyKey: "process-reconstruction:v1")
            ?? { throw JobStoreError.corruptRow }()
        let firstToken = try abandoned.leaseToken ?? { throw JobStoreError.corruptRow }()
        let recoveryTime = Date(timeIntervalSince1970: 2_000)
        try store.reclaimExpiredLeases(now: recoveryTime)
        let recovered = try store.claimDueJobs(
            resourceClass: .embedding,
            capacity: 1,
            now: recoveryTime,
            owner: "recovered-process",
            leaseDuration: 60
        ).first ?? { throw JobStoreError.corruptRow }()
        let token = try recovered.leaseToken ?? { throw JobStoreError.corruptRow }()
        try store.markRunning(id: recovered.id, leaseToken: token, now: recoveryTime)
        try ArtifactRepository(fileURL: databaseURL).commit(
            ArtifactRecord(
                kind: .metadataIndex,
                subjectID: recovered.subjectID,
                inputVersion: recovered.inputVersion,
                outputVersion: "verified-v1",
                contentHash: "verified-v1",
                location: nil,
                origin: "process-harness",
                schemaVersion: 1,
                integrity: .available,
                verifiedAt: recoveryTime
            ),
            completingJobID: recovered.id,
            leaseToken: token
        )
        let complete = try store.job(idempotencyKey: "process-reconstruction:v1")
            ?? { throw JobStoreError.corruptRow }()
        try write(Marker(
            phase: "recover",
            jobID: recovered.id,
            firstLeaseToken: firstToken,
            recoveredLeaseToken: token,
            attempt: complete.attempt,
            state: complete.state
        ), name: "recover.json")
    }

    private static func write(_ marker: Marker, name: String) throws {
        let data = try JSONEncoder().encode(marker)
        try data.write(to: rootURL.appendingPathComponent(name), options: .atomic)
    }

    private static func writeFailure(phase: String, error: Error) throws {
        let data = try JSONSerialization.data(withJSONObject: [
            "phase": phase,
            "error": error.localizedDescription,
        ], options: [.sortedKeys])
        try FileManager.default.createDirectory(at: rootURL, withIntermediateDirectories: true)
        try data.write(to: rootURL.appendingPathComponent("failure.json"), options: .atomic)
    }

    private static var rootURL: URL {
        let support = (try? FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )) ?? FileManager.default.temporaryDirectory
        return support.appendingPathComponent("podcastr/workflow-harness", isDirectory: true)
    }

    private static var databaseURL: URL {
        rootURL.appendingPathComponent("reconstruction.sqlite")
    }
}
#endif
