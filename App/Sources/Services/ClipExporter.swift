import AVFoundation
import CoreImage
import Foundation
import SwiftUI
import UIKit

// MARK: - ClipExporter
//
// Renders a `Clip` to one of three share targets:
//   1. A 1080×1080 PNG card (image).
//   2. A square or 9:16 MP4 with subtitles burned in (video).
//   3. A `podcastr://clip/{id}` deep link (link).
//
// Implemented as an actor so the (long) `AVAssetExportSession` path stays
// off the main thread. The image path bridges back to MainActor explicitly
// (`ImageRenderer` is MainActor-bound). The exporter is stateless beyond
// its temp-file management — a single instance per session is fine.
actor ClipExporter {

    // MARK: - Singleton

    static let shared = ClipExporter()

    // MARK: - Errors

    enum ExportError: Error, CustomStringConvertible {
        /// The clip's source episode is not available locally and streaming
        /// the enclosure URL is disabled for this path. Caller should
        /// download the episode first.
        case audioUnavailable
        /// The video overlay path is intentionally stubbed in this build —
        /// see the share-targets commit message. Image + Link work.
        case notImplemented(String)
        /// AVFoundation reported a failure (composition, export session,
        /// instruction wiring, etc.).
        case avFailure(String)
        /// `ImageRenderer` produced a nil image (typically because the
        /// underlying view tree resolved to zero size).
        case renderFailed

        var description: String {
            switch self {
            case .audioUnavailable:           return "Episode audio is not available locally."
            case .notImplemented(let msg):    return "Not implemented: \(msg)"
            case .avFailure(let msg):         return "AV failure: \(msg)"
            case .renderFailed:               return "Image renderer returned nil."
            }
        }
    }

    // MARK: - Style

    /// Drives subtitle font selection and the image card's pull-quote
    /// rendering. New York serif vs SF Pro semibold per the brief.
    enum SubtitleStyle: String, Sendable, CaseIterable {
        case editorial
        case bold

        var displayName: String {
            switch self {
            case .editorial: return "Editorial"
            case .bold:      return "Bold"
            }
        }
    }

    // MARK: - Public API

    /// Renders the image card and writes it to a temp PNG. Returns the
    /// file URL. Caller is responsible for cleaning up the temp file
    /// (typically left to the OS — `FileManager.default.temporaryDirectory`
    /// is purged automatically).
    func exportImage(
        _ clip: Clip,
        episode: Episode,
        podcast: Podcast,
        theme: SubtitleStyle
    ) async throws -> URL {
        let artwork = await Self.loadArtwork(
            episodeImageURL: episode.imageURL,
            subscriptionImageURL: podcast.imageURL
        )
        let speakerName = clip.speakerID  // Best-effort; sister agent may resolve to display name.
        let timestamp = Self.formatTimestamp(seconds: clip.startSeconds)
        let link = deepLink(clip).absoluteString

        let image = try await MainActor.run { () throws -> UIImage in
            let view = ClipImageCardView(
                showName: podcast.title,
                episodeTitle: episode.title,
                artwork: artwork,
                pullQuote: clip.transcriptText,
                speakerName: speakerName,
                timestamp: timestamp,
                deepLink: link,
                style: theme
            )
            let renderer = ImageRenderer(content: view)
            renderer.scale = 1.0  // The card is already authored at 1080×1080.
            guard let ui = renderer.uiImage else { throw ExportError.renderFailed }
            return ui
        }

        guard let data = image.pngData() else { throw ExportError.renderFailed }
        let url = Self.tempFileURL(prefix: "clip-\(clip.id.uuidString)", ext: "png")
        try data.write(to: url, options: .atomic)
        return url
    }

    /// Builds the deep link for sharing. Stable: `podcastr://clip/{uuid}`.
    nonisolated func deepLink(_ clip: Clip) -> URL {
        URL(string: "podcastr://clip/\(clip.id.uuidString)")!
    }

    /// Trims the episode's local audio to the clip's span and writes a
    /// temp `.m4a` (AAC). The local-file precondition matches the video
    /// path — see `ClipAudioComposer` for rationale.
    func exportAudio(
        _ clip: Clip,
        episode: Episode,
        podcast: Podcast
    ) async throws -> URL {
        try await ClipAudioComposer.export(
            clip: clip,
            episode: episode,
            podcast: podcast
        )
    }

    /// Renders the audio segment + subtitle-burned video. v1 stubs this
    /// when AVFoundation wiring isn't ready in the build window — see
    /// commit message. Wires through `ClipVideoOverlayLayer` for the
    /// subtitle CALayer.
    func exportVideo(
        _ clip: Clip,
        episode: Episode,
        podcast: Podcast,
        theme: SubtitleStyle,
        aspectRatio: ClipVideo.Aspect
    ) async throws -> URL {
        try await ClipVideoComposer.export(
            clip: clip,
            episode: episode,
            podcast: podcast,
            theme: theme,
            aspectRatio: aspectRatio,
            artworkProvider: { @Sendable in
                await Self.loadArtwork(
                    episodeImageURL: episode.imageURL,
                    subscriptionImageURL: podcast.imageURL
                )
            }
        )
    }

    // MARK: - Helpers (file-private statics)

    /// Resolves a temp file URL with a unique name. Always lives in
    /// `FileManager.default.temporaryDirectory` so iOS can sweep it.
    static func tempFileURL(prefix: String, ext: String) -> URL {
        let name = "\(prefix)-\(UUID().uuidString.prefix(8)).\(ext)"
        return FileManager.default.temporaryDirectory.appendingPathComponent(name)
    }

    /// Formats seconds as `H:MM:SS` or `MM:SS` for the share card.
    static func formatTimestamp(seconds: TimeInterval) -> String {
        let total = Int(seconds.rounded(.down))
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        return h > 0
            ? String(format: "%d:%02d:%02d", h, m, s)
            : String(format: "%d:%02d", m, s)
    }

    /// Best-effort artwork fetch: try the episode's image, fall back to
    /// the subscription's, then nil. Keeps `ClipImageCardView` deterministic.
    static func loadArtwork(episodeImageURL: URL?, subscriptionImageURL: URL?) async -> UIImage? {
        for url in [episodeImageURL, subscriptionImageURL].compactMap({ $0 }) {
            if let data = try? await URLSession.shared.data(from: url).0,
               let img = UIImage(data: data) {
                return img
            }
        }
        return nil
    }
}

// MARK: - ClipVideo namespace

/// Caseless namespace for video-export-specific types. Lives next to
/// `ClipExporter` so callers reach for `ClipVideo.Aspect` without an
/// extra import dance.
enum ClipVideo {
    enum Aspect: String, Sendable, CaseIterable {
        case square
        case vertical9x16

        var displayName: String {
            switch self {
            case .square:        return "Square"
            case .vertical9x16:  return "9:16"
            }
        }

        /// Render-target pixel dimensions. 1080-wide canvas matches the
        /// image card and keeps file sizes reasonable for share sheets.
        var renderSize: CGSize {
            switch self {
            case .square:        return CGSize(width: 1080, height: 1080)
            case .vertical9x16:  return CGSize(width: 1080, height: 1920)
            }
        }
    }
}
