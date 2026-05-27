import Foundation
import os.log

// MARK: - DownloadCapability storage helpers
//
// On-disk paths the executor uses for:
//   * downloaded enclosures      — `<Application Support>/Downloads/<episode_id>.mp3`
//   * resume blobs for pause/resume — `<Application Support>/Downloads/.resume/<episode_id>.data`
//
// The path scheme matches the legacy iOS app on purpose. The M4-Rust
// follow-up ("adopt pre-existing legacy downloads — emit `Completed` for
// them on first launch") depends on it, and the M4 milestone pre-flight
// flags `Application Support/Downloads/` as the catalogue location.
//
// All accessors are `static` because the delegate (`+Delegate.swift`)
// runs off the main actor and must move the temp file synchronously
// inside `urlSession(_:downloadTask:didFinishDownloadingTo:)` before
// the OS reclaims the URL. Pure functions over `FileManager` keep the
// concurrency boundary clean.

extension DownloadCapability {
    /// Canonical downloads directory:
    /// `<Application Support>/Downloads/`.
    nonisolated static func downloadsDirectory() -> URL {
        let appSupport: URL
        do {
            appSupport = try FileManager.default.url(
                for: .applicationSupportDirectory,
                in: .userDomainMask,
                appropriateFor: nil,
                create: true)
        } catch {
            // `applicationSupportDirectory` is documented to always
            // exist for app bundles; if we can't reach it, fall back to
            // the Caches directory so reports still carry a real path.
            // The kernel surfaces this as a `Failed` next time the user
            // attempts to play the file.
            appSupport = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        }
        let dir = appSupport.appendingPathComponent("Downloads", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Resume-blob directory:
    /// `<Application Support>/Downloads/.resume/`.
    nonisolated static func resumeDataDirectory() -> URL {
        let dir = downloadsDirectory().appendingPathComponent(".resume", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Canonical destination for a finished download. The filename keeps
    /// the URL's extension when present (so `.m4a` / `.mp3` survive the
    /// move); falls back to `.mp3` because the vast majority of podcast
    /// enclosures are MP3.
    nonisolated static func destinationURL(for episodeID: String, sourceURL: URL?) -> URL {
        let dir = downloadsDirectory()
        let ext: String = {
            guard let raw = sourceURL?.pathExtension, !raw.isEmpty else { return "mp3" }
            // Sanitise: an enclosure URL extension may carry a query
            // string in odd CDN URLs; `URL.pathExtension` already
            // strips the query, but trim further to letters/digits as a
            // belt-and-braces.
            let clean = raw.lowercased().filter { $0.isLetter || $0.isNumber }
            return clean.isEmpty ? "mp3" : clean
        }()
        let safeID = episodeID.replacingOccurrences(of: "/", with: "_")
        return dir.appendingPathComponent("\(safeID).\(ext)")
    }

    /// Path the executor uses for resume data persistence. Keyed by
    /// `episode_id` so `PauseDownload` → app suspension → app launch →
    /// `ResumeDownload` survives a kill cleanly.
    nonisolated static func resumeDataURL(for episodeID: String) -> URL {
        let safeID = episodeID.replacingOccurrences(of: "/", with: "_")
        return resumeDataDirectory().appendingPathComponent("\(safeID).data")
    }

    /// Persist resume data for a paused download. Quiet on I/O failure
    /// — Rust will surface a `Failed` next `ResumeDownload` if the blob
    /// disappears.
    nonisolated static func writeResumeData(_ data: Data, for episodeID: String) {
        let url = resumeDataURL(for: episodeID)
        try? data.write(to: url, options: .atomic)
    }

    /// Load resume data for an episode if one was previously stashed.
    nonisolated static func loadResumeData(for episodeID: String) -> Data? {
        let url = resumeDataURL(for: episodeID)
        return try? Data(contentsOf: url)
    }

    /// Drop a previously-stashed resume blob. Called on cancel and on
    /// successful completion so the directory doesn't accumulate stale
    /// blobs across kill/relaunch cycles.
    nonisolated static func clearResumeData(for episodeID: String) {
        let url = resumeDataURL(for: episodeID)
        try? FileManager.default.removeItem(at: url)
    }

    /// Move a freshly-downloaded temp file into its canonical location.
    /// Caller is responsible for invoking this *synchronously* inside
    /// the `didFinishDownloadingTo` delegate so the temp URL is still
    /// valid. Returns the destination URL on success, `nil` on I/O
    /// failure (caller emits `Failed` from the resulting `nil`).
    nonisolated static func moveFinishedDownload(
        from tempURL: URL,
        episodeID: String,
        sourceURL: URL?
    ) -> URL? {
        let destination = destinationURL(for: episodeID, sourceURL: sourceURL)
        let fm = FileManager.default
        do {
            // Defensive: clear any pre-existing file at destination.
            if fm.fileExists(atPath: destination.path) {
                try fm.removeItem(at: destination)
            }
            try fm.moveItem(at: tempURL, to: destination)
            // Successful move ⇒ resume blob is moot. Drop it so the
            // next `StartDownload` for the same id starts fresh.
            clearResumeData(for: episodeID)
            return destination
        } catch {
            return nil
        }
    }
}
