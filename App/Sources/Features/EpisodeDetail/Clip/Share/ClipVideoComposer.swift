import AVFoundation
import Foundation
import QuartzCore
import UIKit

// MARK: - ClipVideoComposer
//
// **Status: STUBBED** — `export(...)` throws `.notImplemented` per the
// share-targets brief's punt clause. The Image + Link share targets are
// fully wired; video is the explicit long pole that doesn't fit in this
// branch's budget. Image card + deep link both ship working.
//
// What we'd need for a real implementation:
//
//   1. **Generator video track.** `AVVideoCompositionCoreAnimationTool`
//      requires actual frames flowing through the composition's video
//      track to drive timeline progress. An empty composition track
//      compiles but yields `AVErrorInvalidVideoComposition` at export
//      time. Two viable shapes:
//        a. Pre-render a 1-frame H.264 .mov from the artwork via
//           `AVAssetWriter`, then `insertTimeRange(...)` it stretched
//           to the clip duration (lowest CPU; brittle if the still
//           track desyncs from audio).
//        b. Build a `AVMutableVideoComposition.videoComposition(asset:)`
//           with the audio-only asset and feed in a custom
//           `AVVideoCompositionInstruction` that paints from the
//           CALayer hierarchy each frame.
//
//   2. **Audio source.** Local-only via
//      `EpisodeDownloadStore.shared.localFileURL(for:)`; throw
//      `ClipExporter.ExportError.audioUnavailable` when the file is
//      missing. Streaming the enclosure would work but makes export
//      time unpredictable; better to surface a "download first" error.
//
//   3. **Subtitle layer.** `ClipVideoOverlayLayer.makeOverlay(cues:)`
//      already produces the CALayer hierarchy. Sentence-level reveal
//      via `ClipVideoOverlayLayer.sentenceCues(text:duration:)` is
//      plenty for v1 — word-level animation is the stretch goal noted
//      in the brief.
//
//   4. **Export.** `AVAssetExportPresetHighestQuality` to a temp .mp4
//      with `videoComposition` set + `shouldOptimizeForNetworkUse =
//      true`. Modern `await session.export()` is iOS 18+; deployment
//      target is iOS 26 so the async API is fine.
//
// The helpers below (`resolveLocalAudioURL`, `makeBackgroundLayer`) are
// kept because they're correct in isolation and the next pass will
// need them. Removing them now would force a rewrite when the real
// implementation lands.
enum ClipVideoComposer {

    // MARK: - Public entry point

    static func export(
        clip: Clip,
        episode: Episode,
        podcast: Podcast,
        theme: ClipExporter.SubtitleStyle,
        aspectRatio: ClipVideo.Aspect,
        artworkProvider: @Sendable () async -> UIImage?
    ) async throws -> URL {
        // Surface the audio precondition early so the punt error is
        // less noisy when the user wouldn't have been able to render
        // anyway. Keeps parity with what the real implementation will
        // require (download-first).
        _ = try resolveLocalAudioURL(for: episode)
        _ = aspectRatio
        _ = theme
        _ = clip
        _ = podcast
        _ = artworkProvider

        throw ClipExporter.ExportError.notImplemented(
            "Video export is pending generator-track wiring; see ClipVideoComposer header."
        )
    }

    // MARK: - Audio resolution (used by the real export path; kept stable)

    private static func resolveLocalAudioURL(for episode: Episode) throws -> URL {
        let store = EpisodeDownloadStore.shared
        guard store.exists(for: episode) else {
            throw ClipExporter.ExportError.audioUnavailable
        }
        return store.localFileURL(for: episode)
    }

    // MARK: - Background layer (used by the real export path; kept stable)

    /// Builds the static backdrop CALayer for the video overlay. Static
    /// dimmed artwork (or brand gradient) + bottom vignette + show-name
    /// watermark. Matches the still-image card's mood so the two share
    /// targets read as a set.
    static func makeBackgroundLayer(
        size: CGSize,
        artwork: UIImage?,
        podcast: Podcast
    ) -> CALayer {
        let bg = CALayer()
        bg.frame = CGRect(origin: .zero, size: size)
        bg.backgroundColor = UIColor.black.cgColor

        if let artwork, let cg = artwork.cgImage {
            let art = CALayer()
            art.frame = CGRect(origin: .zero, size: size)
            art.contents = cg
            art.contentsGravity = .resizeAspectFill
            art.opacity = 0.35
            bg.addSublayer(art)
        } else {
            let grad = CAGradientLayer()
            grad.frame = CGRect(origin: .zero, size: size)
            grad.colors = [
                UIColor.systemOrange.withAlphaComponent(0.75).cgColor,
                UIColor.systemPurple.withAlphaComponent(0.65).cgColor
            ]
            grad.startPoint = CGPoint(x: 0, y: 0)
            grad.endPoint = CGPoint(x: 1, y: 1)
            bg.addSublayer(grad)
        }

        // Vignette: dim the bottom band so subtitles always have contrast.
        let vignette = CAGradientLayer()
        vignette.frame = CGRect(
            x: 0,
            y: size.height * 0.55,
            width: size.width,
            height: size.height * 0.45
        )
        vignette.colors = [
            UIColor.black.withAlphaComponent(0.0).cgColor,
            UIColor.black.withAlphaComponent(0.65).cgColor
        ]
        vignette.startPoint = CGPoint(x: 0.5, y: 0)
        vignette.endPoint = CGPoint(x: 0.5, y: 1)
        bg.addSublayer(vignette)

        // Show-name watermark at the top.
        let label = CATextLayer()
        let font = UIFont.systemFont(ofSize: size.height * 0.022, weight: .semibold)
        label.string = NSAttributedString(
            string: podcast.title.uppercased(),
            attributes: [
                .font: font,
                .foregroundColor: UIColor.white.withAlphaComponent(0.78),
                .kern: 2.0
            ]
        )
        label.alignmentMode = .center
        label.contentsScale = UIScreen.main.scale
        label.frame = CGRect(
            x: 0,
            y: size.height * 0.06,
            width: size.width,
            height: size.height * 0.04
        )
        bg.addSublayer(label)

        return bg
    }
}
