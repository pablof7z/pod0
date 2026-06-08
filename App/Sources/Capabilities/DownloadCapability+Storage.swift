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

    /// On-device LLM models directory:
    /// `<Application Support>/LocalModels/`.
    ///
    /// Byte-for-byte the same location the (now-removed) `LocalModelDownloadManager`
    /// used, so models already on disk are found without a re-download.
    nonisolated static func localModelsDirectory() -> URL {
        let appSupport: URL
        do {
            appSupport = try FileManager.default.url(
                for: .applicationSupportDirectory,
                in: .userDomainMask,
                appropriateFor: nil,
                create: true)
        } catch {
            appSupport = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        }
        let dir = appSupport.appendingPathComponent("LocalModels", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Canonical on-disk path of a local model file:
    /// `<Application Support>/LocalModels/<id>.litertlm`. The single source of
    /// truth for "is this model downloaded" (read by `LocalLLMService.load` and
    /// `syncLocalEngine`) — matches the unified download executor's destination.
    nonisolated static func localModelFileURL(for modelID: String) -> URL {
        let safeID = modelID.replacingOccurrences(of: "/", with: "_")
        return localModelsDirectory().appendingPathComponent("\(safeID).litertlm")
    }

    /// Canonical on-disk path of a downloaded Core ML embedding model:
    /// `<Application Support>/LocalModels/<id>.mlpackage`. Distinct from the
    /// `.litertlm` LLM helper above because Core ML compiles `.mlpackage`
    /// bundles (not single weight blobs); kept separate so each model kind has
    /// one canonical destination. Read by `CoreMLEmbeddingProvider` to decide
    /// readiness (issue #236).
    nonisolated static func embeddingModelFileURL(for modelID: String) -> URL {
        let safeID = modelID.replacingOccurrences(of: "/", with: "_")
        return localModelsDirectory().appendingPathComponent("\(safeID).mlpackage")
    }

    /// Resume-blob directory:
    /// `<Application Support>/Downloads/.resume/`.
    nonisolated static func resumeDataDirectory() -> URL {
        let dir = downloadsDirectory().appendingPathComponent(".resume", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Canonical destination for a finished episode download (back-compat
    /// wrapper). See the kind-aware overload.
    nonisolated static func destinationURL(for episodeID: String, sourceURL: URL?) -> URL {
        destinationURL(for: episodeID, sourceURL: sourceURL, kind: .episode)
    }

    /// Canonical destination for a finished download.
    ///
    /// * `.episode` → `Downloads/<id>.<ext>` — the filename keeps the URL's
    ///   extension when present (so `.m4a` / `.mp3` survive the move); falls
    ///   back to `.mp3` (the vast majority of enclosures are MP3).
    /// * `.localModel` → `LocalModels/<id>.litertlm` — fixed extension; the
    ///   source URL's extension is irrelevant.
    nonisolated static func destinationURL(for episodeID: String, sourceURL: URL?, kind: DownloadKind) -> URL {
        if kind == .localModel {
            return localModelFileURL(for: episodeID)
        }
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

    /// Path the executor uses for resume data persistence. Keyed by id (and,
    /// for non-episodes, a kind prefix so a model id can never alias an episode
    /// blob) so `PauseDownload` → app suspension → app launch → `ResumeDownload`
    /// survives a kill cleanly. Episodes keep the bare `<id>.data` name for
    /// back-compat with blobs stashed by earlier builds.
    nonisolated static func resumeDataURL(for episodeID: String, kind: DownloadKind = .episode) -> URL {
        let safeID = episodeID.replacingOccurrences(of: "/", with: "_")
        let name = kind == .episode ? "\(safeID).data" : "\(kind.rawValue)_\(safeID).data"
        return resumeDataDirectory().appendingPathComponent(name)
    }

    /// Persist resume data for a paused download. Quiet on I/O failure
    /// — Rust will surface a `Failed` next `ResumeDownload` if the blob
    /// disappears.
    nonisolated static func writeResumeData(_ data: Data, for episodeID: String, kind: DownloadKind = .episode) {
        let url = resumeDataURL(for: episodeID, kind: kind)
        try? data.write(to: url, options: .atomic)
    }

    /// Load resume data for an item if one was previously stashed.
    nonisolated static func loadResumeData(for episodeID: String, kind: DownloadKind = .episode) -> Data? {
        let url = resumeDataURL(for: episodeID, kind: kind)
        return try? Data(contentsOf: url)
    }

    /// Drop a previously-stashed resume blob. Called on cancel and on
    /// successful completion so the directory doesn't accumulate stale
    /// blobs across kill/relaunch cycles.
    nonisolated static func clearResumeData(for episodeID: String, kind: DownloadKind = .episode) {
        let url = resumeDataURL(for: episodeID, kind: kind)
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
        sourceURL: URL?,
        kind: DownloadKind = .episode
    ) -> URL? {
        let destination = destinationURL(for: episodeID, sourceURL: sourceURL, kind: kind)
        let fm = FileManager.default
        do {
            // Defensive: clear any pre-existing file at destination.
            if fm.fileExists(atPath: destination.path) {
                try fm.removeItem(at: destination)
            }
            try fm.moveItem(at: tempURL, to: destination)
            // Successful move ⇒ resume blob is moot. Drop it so the
            // next `StartDownload` for the same id starts fresh.
            clearResumeData(for: episodeID, kind: kind)
            return destination
        } catch {
            return nil
        }
    }
}
