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
//   - The reader, the wiki generator, and the agent all need the same
//     parsed transcript; one file serves all consumers.
//
// Thread-safety: the disk surface is conservative — synchronous on the
// caller's actor. The class is `@unchecked Sendable` because the only
// mutable state is `rootURL`, which is set once at init.

final class TranscriptStore: @unchecked Sendable {

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

    /// Read the transcript for `episodeID`, or `nil` if none has been
    /// persisted.
    func load(episodeID: UUID) -> Transcript? {
        let url = fileURL(for: episodeID)
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

    private func fileURL(for episodeID: UUID) -> URL {
        rootURL.appendingPathComponent("\(episodeID.uuidString).json")
    }
}
