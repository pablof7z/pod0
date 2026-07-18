import Foundation
import os.log

// MARK: - TranscriptStore
//
// Persists parsed `Transcript` JSON to disk under
// `$applicationSupport/podcastr/transcripts/<episodeID>.json` and serves
// them back by episode ID for the EpisodeDetail / Reader views.
//
// Why a dedicated store rather than a column on the `Episode` model:
//   - Transcripts can run hundreds of KB; we don't want them in the
//     periodically-saved `AppState` json blob.
//   - The reader and the agent both need the same parsed transcript; one
//     file serves all consumers.
//
// Thread-safety: the disk surface is conservative — synchronous on the
// caller's actor. The class is `@unchecked Sendable` because the only
// mutable state is `rootURL`, which is set once at init.

final class TranscriptStore: @unchecked Sendable {

    struct StagedOutput: Codable, Sendable, Equatable {
        let jobID: UUID
        let leaseToken: UUID
        let episodeID: UUID
        let inputVersion: String
        let contentHash: String
        let filePath: String
    }

    // MARK: Singleton

    static let shared: TranscriptStore = {
        do {
            return try TranscriptStore()
        } catch {
            // Fall back to the temporary directory so the app keeps running
            // even if Application Support is unavailable.
            logger.error("Application Support unavailable — TranscriptStore falling back to temporaryDirectory. Transcripts will NOT survive app relaunch. Underlying error: \(String(describing: error), privacy: .public)")
            let tmp = FileManager.default.temporaryDirectory
                .appendingPathComponent("podcastr-transcripts", isDirectory: true)
            // swiftlint:disable:next force_try
            let store = try! TranscriptStore(rootDirectory: tmp)
            store.isUsingEphemeralStorage = true
            return store
        }
    }()

    // MARK: Ephemeral-storage flag

    /// `true` when this instance is writing to `temporaryDirectory` because
    /// `Application Support` was unavailable at launch. Data will be lost on
    /// the next app relaunch.
    private(set) var isUsingEphemeralStorage: Bool = false

    // MARK: Logger

    private static let logger = Logger.app("TranscriptStore")

    // MARK: State

    let rootURL: URL

    // MARK: Init

    init(rootDirectory: URL? = nil) throws {
        if let rootDirectory {
            self.rootURL = rootDirectory
        } else {
            let support = try FileManager.default.url(
                for: .applicationSupportDirectory,
                in: .userDomainMask,
                appropriateFor: nil,
                create: true
            )
            self.rootURL = support
                .appendingPathComponent("podcastr", isDirectory: true)
                .appendingPathComponent("transcripts", isDirectory: true)
        }
        try FileManager.default.createDirectory(
            at: rootURL,
            withIntermediateDirectories: true
        )
    }

    // MARK: API

    /// Write `transcript` to disk, replacing any existing file for the
    /// same episode.
    func save(_ transcript: Transcript) throws {
        let url = fileURL(for: transcript.episodeID)
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(transcript)
        try data.write(to: url, options: .atomic)
        Self.logger.debug(
            "wrote transcript for \(transcript.episodeID, privacy: .public) (\(data.count, privacy: .public) bytes)"
        )
    }

    /// Writes an attempt-owned output. The selected transcript is never
    /// overwritten until the producing lease is fenced in SQLite.
    func stage(_ transcript: Transcript, context: JobAttemptContext) throws -> String {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(transcript)
        let hash = ArtifactRepository.hash(data)
        let url = stagedFileURL(for: transcript.episodeID, leaseToken: context.leaseToken)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true
        )
        try data.write(to: url, options: .atomic)
        let output = StagedOutput(
            jobID: context.job.id,
            leaseToken: context.leaseToken,
            episodeID: transcript.episodeID,
            inputVersion: context.job.inputVersion,
            contentHash: hash,
            filePath: url.path
        )
        try encoder.encode(output).write(to: manifestURL(for: url), options: .atomic)
        return hash
    }

    func verifiedStagedData(episodeID: UUID, leaseToken: UUID) -> Data? {
        verifiedData(at: stagedFileURL(for: episodeID, leaseToken: leaseToken), episodeID: episodeID)
    }

    /// Promotes a verified attempt to an immutable content-addressed path.
    /// A crash after this move is safe: reconciliation can adopt the file.
    func promoteStaged(episodeID: UUID, leaseToken: UUID, contentHash: String) throws -> URL {
        let staged = stagedFileURL(for: episodeID, leaseToken: leaseToken)
        guard let data = verifiedData(at: staged, episodeID: episodeID),
              ArtifactRepository.hash(data) == contentHash else {
            throw JobFailure(classification: .corruptArtifact, message: "Staged transcript failed verification")
        }
        let destination = contentFileURL(for: episodeID, contentHash: contentHash)
        try FileManager.default.createDirectory(
            at: destination.deletingLastPathComponent(), withIntermediateDirectories: true
        )
        if !FileManager.default.fileExists(atPath: destination.path) {
            try data.write(to: destination, options: .withoutOverwriting)
        }
        try? FileManager.default.removeItem(at: staged)
        try? FileManager.default.removeItem(at: manifestURL(for: staged))
        return destination
    }

    func recoverableStagedOutput(episodeID: UUID, inputVersion: String) -> StagedOutput? {
        let directory = rootURL.appendingPathComponent("staging", isDirectory: true)
            .appendingPathComponent(episodeID.uuidString, isDirectory: true)
        guard let urls = try? FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: [.contentModificationDateKey]
        ) else { return nil }
        let decoder = JSONDecoder()
        return urls.filter { $0.pathExtension == "manifest" }.compactMap { url -> StagedOutput? in
            guard let data = try? Data(contentsOf: url),
                  let output = try? decoder.decode(StagedOutput.self, from: data),
                  output.episodeID == episodeID,
                  output.inputVersion == inputVersion,
                  let transcriptData = verifiedData(
                    at: URL(fileURLWithPath: output.filePath), episodeID: episodeID
                  ),
                  ArtifactRepository.hash(transcriptData) == output.contentHash else { return nil }
            return output
        }.sorted { $0.filePath > $1.filePath }.first
    }

    /// Read the transcript for `episodeID`, or `nil` if none has been
    /// persisted.
    func load(episodeID: UUID) -> Transcript? {
        let selected = try? ArtifactRepository(
            fileURL: Persistence.shared.episodeStore.fileURL
        ).current(kind: .transcript, subjectID: episodeID)
        let url = selected?.location.map(URL.init(fileURLWithPath:)) ?? fileURL(for: episodeID)
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        do {
            let data = try Data(contentsOf: url)
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            return try decoder.decode(Transcript.self, from: data)
        } catch {
            Self.logger.error(
                "failed to load transcript for \(episodeID, privacy: .public): \(String(describing: error), privacy: .public)"
            )
            return nil
        }
    }

    /// Delete the persisted transcript for `episodeID`. Idempotent.
    func delete(episodeID: UUID) {
        let url = fileURL(for: episodeID)
        try? FileManager.default.removeItem(at: url)
    }

    // MARK: Helpers

    func fileURL(for episodeID: UUID) -> URL {
        rootURL.appendingPathComponent("\(episodeID.uuidString).json")
    }

    func stagedFileURL(for episodeID: UUID, leaseToken: UUID) -> URL {
        rootURL.appendingPathComponent("staging", isDirectory: true)
            .appendingPathComponent(episodeID.uuidString, isDirectory: true)
            .appendingPathComponent("\(leaseToken.uuidString).json")
    }

    private func manifestURL(for stagedURL: URL) -> URL {
        stagedURL.deletingPathExtension().appendingPathExtension("manifest")
    }

    func contentFileURL(for episodeID: UUID, contentHash: String) -> URL {
        rootURL.appendingPathComponent("artifacts", isDirectory: true)
            .appendingPathComponent(episodeID.uuidString, isDirectory: true)
            .appendingPathComponent("\(contentHash).json")
    }

    func verifiedData(episodeID: UUID) -> Data? {
        let selected = try? ArtifactRepository(
            fileURL: Persistence.shared.episodeStore.fileURL
        ).current(kind: .transcript, subjectID: episodeID)
        let url = selected?.location.map(URL.init(fileURLWithPath:)) ?? fileURL(for: episodeID)
        return verifiedData(at: url, episodeID: episodeID)
    }

    func verifiedData(at url: URL, episodeID: UUID) -> Data? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        guard let transcript = try? decoder.decode(Transcript.self, from: data),
              transcript.episodeID == episodeID else { return nil }
        return data
    }
}
