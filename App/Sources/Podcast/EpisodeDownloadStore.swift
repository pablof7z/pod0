import Foundation
import os.log

// MARK: - EpisodeDownloadStore

/// Helper that owns the on-disk layout for downloaded episode enclosures.
///
/// Files live under `$applicationSupport/podcastr/downloads/<episodeID>.<ext>`.
/// The path is deterministic in the episode `id` so a re-launched app can
/// recompute it without trusting any URL we may have persisted in the
/// `DownloadState.downloaded(localFileURL:)` payload — iOS may rotate the app
/// container path, so absolute file URLs from a previous session are brittle.
///
/// This type is intentionally tiny: pure path math + file existence checks +
/// directory bootstrapping. The `EpisodeDownloadService` orchestrates the
/// network layer on top.
final class EpisodeDownloadStore: @unchecked Sendable {

    // MARK: Singleton

    static let shared: EpisodeDownloadStore = {
        do {
            return try EpisodeDownloadStore()
        } catch {
            // Fall back to the temporary directory so the app keeps running
            // even if Application Support is unavailable. Mirrors TranscriptStore.
            let tmp = FileManager.default.temporaryDirectory
                .appendingPathComponent("podcastr-downloads", isDirectory: true)
            // swiftlint:disable:next force_try
            return try! EpisodeDownloadStore(rootDirectory: tmp)
        }
    }()

    // MARK: Logger

    private static let logger = Logger.app("EpisodeDownloadStore")

    // MARK: State

    /// Root directory where every downloaded enclosure lives.
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
                .appendingPathComponent("downloads", isDirectory: true)
        }
        try FileManager.default.createDirectory(
            at: rootURL,
            withIntermediateDirectories: true
        )
    }

    // MARK: - Path math

    /// Deterministic on-disk URL for `episode`. Does not check existence.
    func localFileURL(for episode: Episode) -> URL {
        rootURL.appendingPathComponent("\(episode.id.uuidString).\(fileExtension(for: episode))")
    }

    /// Resume-data sidecar for `episode`. Used by the service to persist
    /// `URLSession` resume blobs across app launches when a download is
    /// interrupted (e.g. user kills the app mid-fetch).
    func resumeDataURL(for episode: Episode) -> URL {
        rootURL.appendingPathComponent("\(episode.id.uuidString).resume")
    }

    /// `true` when a previously-downloaded file is still on disk.
    func exists(for episode: Episode) -> Bool {
        FileManager.default.fileExists(atPath: localFileURL(for: episode).path)
    }

    /// Size of the downloaded file in bytes, or `nil` when the file is missing
    /// or the attribute is unavailable.
    func byteCount(for episode: Episode) -> Int64? {
        let url = localFileURL(for: episode)
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: url.path) else {
            return nil
        }
        return (attrs[.size] as? NSNumber)?.int64Value
    }

    /// Removes the local file for `episode` if present. Idempotent — missing
    /// files are not treated as errors.
    func delete(for episode: Episode) throws {
        let url = localFileURL(for: episode)
        let resumeURL = resumeDataURL(for: episode)
        let fm = FileManager.default
        if fm.fileExists(atPath: url.path) {
            try fm.removeItem(at: url)
        }
        if fm.fileExists(atPath: resumeURL.path) {
            try? fm.removeItem(at: resumeURL)
        }
    }

    /// Persists `resumeData` so a future launch can pick the download back up.
    func writeResumeData(_ resumeData: Data, for episode: Episode) {
        let url = resumeDataURL(for: episode)
        do {
            try resumeData.write(to: url, options: .atomic)
        } catch {
            Self.logger.error(
                "writeResumeData failed for \(episode.id, privacy: .public): \(error, privacy: .public)"
            )
        }
    }

    /// Returns previously-saved resume data for `episode`, or `nil` if absent.
    func loadResumeData(for episode: Episode) -> Data? {
        let url = resumeDataURL(for: episode)
        return try? Data(contentsOf: url)
    }

    /// Removes the resume sidecar (called once the download lands successfully).
    func clearResumeData(for episode: Episode) {
        let url = resumeDataURL(for: episode)
        try? FileManager.default.removeItem(at: url)
    }

    // MARK: - Disk-usage aggregation
    //
    // These walk `rootURL` directly — *not* the `state.episodes` list — so
    // orphaned files (downloads whose Episode has been removed from the
    // store, e.g. unsubscribed shows) still get counted toward the user's
    // total. The `StorageSettingsView` reconciles the directory walk
    // against the live episode list to label each entry.

    /// One on-disk artifact: either an enclosure (`.mp3` etc.) or a
    /// `.resume` sidecar from an interrupted download.
    struct OnDiskFile: Sendable {
        let url: URL
        let bytes: Int64
        /// Parsed `Episode.id` — every file is named `<uuid>.<ext>` so we
        /// can join back to the live store. `nil` for malformed file names
        /// the user (or a previous build) might have left behind.
        let episodeID: UUID?
        let isResumeData: Bool
    }

    /// Walks the downloads directory and returns every artifact on disk —
    /// enclosures + `.resume` sidecars. Pure I/O, safe to call off the main
    /// thread; the caller is responsible for caching the result rather than
    /// re-walking on every redraw.
    func enumerateOnDisk() -> [OnDiskFile] {
        let fm = FileManager.default
        guard let entries = try? fm.contentsOfDirectory(
            at: rootURL,
            includingPropertiesForKeys: [.fileSizeKey],
            options: [.skipsHiddenFiles]
        ) else {
            return []
        }
        return entries.compactMap { url -> OnDiskFile? in
            let attrs = try? fm.attributesOfItem(atPath: url.path)
            let bytes = (attrs?[.size] as? NSNumber)?.int64Value ?? 0
            let stem = url.deletingPathExtension().lastPathComponent
            let episodeID = UUID(uuidString: stem)
            let isResume = url.pathExtension.lowercased() == "resume"
            return OnDiskFile(
                url: url,
                bytes: bytes,
                episodeID: episodeID,
                isResumeData: isResume
            )
        }
    }

    // MARK: - Extension inference

    /// Infers a usable file extension from the enclosure URL or its MIME type.
    /// Defaults to `mp3` because the overwhelming majority of podcast feeds
    /// ship MP3 and the suffix matters mostly for `AVURLAsset`'s sniffing.
    private func fileExtension(for episode: Episode) -> String {
        let pathExt = episode.enclosureURL.pathExtension.lowercased()
        if !pathExt.isEmpty, pathExt.count <= 5 {
            return pathExt
        }
        switch episode.enclosureMimeType?.lowercased() {
        case "audio/mp4", "audio/x-m4a", "audio/aac": return "m4a"
        case "audio/mpeg", "audio/mp3":               return "mp3"
        case "audio/wav", "audio/x-wav":              return "wav"
        case "audio/ogg":                             return "ogg"
        case "audio/opus":                            return "opus"
        default:                                      return "mp3"
        }
    }
}
