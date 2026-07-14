import AVFoundation
import Foundation

// MARK: - ClipAudioComposer
//
// Third clip-share fidelity (Image / Video / Audio + universal link). The
// image card and link already ship; video is intentionally stubbed in
// `ClipVideoComposer`. Audio is the cheapest of the three to do right
// because the heavy lifting belongs to `AVAssetExportSession` — no
// compositing, no overlay layers, no generator video track.
//
// Pipeline:
//   1. Resolve the episode's local audio file. Streaming the enclosure
//      is intentionally NOT supported here — same precondition the video
//      composer documents. Reasons: unpredictable export time, no
//      progress signal, and the user can always download first.
//   2. Build an `AVURLAsset` over the local file and an
//      `AVAssetExportSession` with `presetAppleM4A` (AAC in an m4a
//      container — universal iOS / Mac / web compatibility, no licensing
//      drama).
//   3. Set `timeRange` to the clip's `[startMs, endMs]` span. AVFoundation
//      handles the clipping; we get a self-contained file at the output
//      URL when the session reports `.completed`.
//   4. Optional `metadata`: title (caption or first 60 chars of transcript)
//      + album artist (show name). Players show this in the file picker.
//
// Why m4a (not mp3): no encoder licensing concerns, native iOS Files /
// Voice Memos / Messages all handle it, and `presetAppleM4A` is a
// first-class AVFoundation path with predictable output. The pure
// passthrough preset would skip re-encoding but doesn't handle a
// trimmed time range, so we accept the AAC re-encode.
enum ClipAudioComposer {

    // MARK: - Public entry point

    /// Export `clip`'s audio span to a temp `.m4a` file.
    ///
    /// - Returns: File URL inside `FileManager.default.temporaryDirectory`
    ///   suitable for handing directly to `ShareLink(item:)`.
    /// - Throws: `ClipExporter.ExportError.audioUnavailable` when the
    ///   episode hasn't been downloaded; `.avFailure` on any AVFoundation
    ///   error during export.
    static func export(
        clip: Clip,
        episode: Episode,
        podcast: Podcast
    ) async throws -> URL {
        let sourceURL = try resolveLocalAudioURL(for: episode)
        let asset = AVURLAsset(url: sourceURL)

        guard let session = AVAssetExportSession(
            asset: asset,
            presetName: AVAssetExportPresetAppleM4A
        ) else {
            throw ClipExporter.ExportError.avFailure(
                "AVAssetExportSession init failed for \(sourceURL.lastPathComponent)"
            )
        }

        let outputURL = ClipExporter.tempFileURL(
            prefix: "clip-audio-\(clip.id.uuidString)",
            ext: "m4a"
        )
        // Defensive: a leftover file at the same path makes the session
        // fail with `.failed` and a useless underlying error. We picked a
        // UUID-suffixed name above so this shouldn't collide in practice,
        // but cleaning up is free insurance.
        try? FileManager.default.removeItem(at: outputURL)

        session.outputURL = outputURL
        session.outputFileType = .m4a
        session.timeRange = CMTimeRange(
            start: CMTime(seconds: clip.startSeconds, preferredTimescale: 600),
            duration: CMTime(seconds: clip.durationSeconds, preferredTimescale: 600)
        )
        session.metadata = buildMetadata(clip: clip, episode: episode, podcast: podcast)

        // iOS 18+ async export. Deployment target is iOS 26 so the
        // legacy `exportAsynchronously(completionHandler:)` callback
        // version isn't worth carrying.
        try await session.export(to: outputURL, as: .m4a)
        return outputURL
    }

    // MARK: - Helpers

    /// Same precondition shape as `ClipVideoComposer.resolveLocalAudioURL`.
    /// Kept independent (rather than reaching across files) so the
    /// audio path stays self-contained and the video stub is free to
    /// evolve without coupling them.
    private static func resolveLocalAudioURL(for episode: Episode) throws -> URL {
        let store = EpisodeDownloadStore.shared
        guard store.exists(for: episode) else {
            throw ClipExporter.ExportError.audioUnavailable
        }
        return store.localFileURL(for: episode)
    }

    /// Metadata embedded in the exported `.m4a` so when the recipient
    /// opens the file in Voice Memos / Music / Files, they see *which*
    /// show + episode the clip came from rather than a bare UUID
    /// filename.
    private static func buildMetadata(
        clip: Clip,
        episode: Episode,
        podcast: Podcast
    ) -> [AVMetadataItem] {
        var items: [AVMetadataItem] = []

        // Title: caption if the user wrote one; otherwise truncate the
        // captured prose. iOS' file row truncates at ~60 chars anyway.
        let title: String = {
            if let c = clip.caption, !c.isEmpty { return c }
            let trimmed = clip.transcriptText.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.isEmpty { return episode.title }
            if trimmed.count <= 60 { return trimmed }
            return String(trimmed.prefix(57)) + "…"
        }()
        items.append(makeMetadataItem(identifier: .commonIdentifierTitle, value: title))
        items.append(makeMetadataItem(identifier: .commonIdentifierArtist, value: podcast.title))
        items.append(makeMetadataItem(identifier: .commonIdentifierAlbumName, value: episode.title))
        return items
    }

    private static func makeMetadataItem(
        identifier: AVMetadataIdentifier,
        value: String
    ) -> AVMetadataItem {
        let item = AVMutableMetadataItem()
        item.identifier = identifier
        item.value = value as NSString
        item.extendedLanguageTag = "und"  // Locale-agnostic — show name + title aren't language-specific.
        return item
    }
}
