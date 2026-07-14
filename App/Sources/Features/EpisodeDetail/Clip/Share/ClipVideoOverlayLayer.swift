import Foundation
import QuartzCore
import UIKit

// MARK: - ClipVideoOverlayLayer
//
// Builds the burn-in subtitle CALayer hierarchy used by
// `AVVideoCompositionCoreAnimationTool`. v1 ships sentence-by-sentence
// reveal (one CATextLayer per sentence; each fades in / out at its
// timestamp). Word-level animation is the stretch goal noted in the brief.
//
// All times in the input segments are seconds **relative to the video
// timeline** (i.e. clip-local: 0..durationSeconds), not the original
// episode timeline. The exporter is responsible for translating
// `Clip.startSeconds`-anchored words to clip-local time before calling
// `makeOverlay(...)`.
enum ClipVideoOverlayLayer {

    // MARK: - Input

    /// One subtitle line — the smallest unit we animate. v1 emits one of
    /// these per sentence; future passes can split per word and feed in
    /// many cues to get karaoke-style reveal without changing the
    /// `makeOverlay` signature.
    struct Cue: Sendable, Hashable {
        let text: String
        let start: TimeInterval
        let end: TimeInterval
    }

    /// Visual configuration. Driven by `ClipExporter.SubtitleStyle` and the
    /// rendered video's pixel size so type scales with the canvas.
    struct Config: Sendable {
        let renderSize: CGSize
        let style: ClipExporter.SubtitleStyle
        let speakerName: String?

        var fontSize: CGFloat { renderSize.height * 0.045 }
        var horizontalInset: CGFloat { renderSize.width * 0.08 }
        var bottomInset: CGFloat { renderSize.height * 0.12 }
    }

    // MARK: - API

    /// Returns the parent CALayer that
    /// `AVVideoCompositionCoreAnimationTool` expects: a container at the
    /// full render size containing animated text overlays. The animations
    /// are pre-baked with `beginTime` set to `AVCoreAnimationBeginTimeAtZero`-
    /// compatible values; the caller still has to wrap the parent in a
    /// `CALayer` of identical bounds for the tool API.
    static func makeOverlay(cues: [Cue], config: Config) -> CALayer {
        let parent = CALayer()
        parent.frame = CGRect(origin: .zero, size: config.renderSize)
        parent.isGeometryFlipped = false

        if let speaker = config.speakerName, !speaker.isEmpty {
            parent.addSublayer(speakerLabelLayer(name: speaker, config: config))
        }

        for cue in cues {
            parent.addSublayer(textLayer(for: cue, config: config))
        }
        return parent
    }

    // MARK: - Layer factories

    private static func textLayer(for cue: Cue, config: Config) -> CATextLayer {
        let layer = CATextLayer()
        layer.string = attributedString(for: cue.text, config: config)
        layer.alignmentMode = .center
        layer.contentsScale = UIScreen.main.scale
        layer.isWrapped = true
        layer.allowsFontSubpixelQuantization = true
        layer.frame = subtitleFrame(in: config)
        layer.opacity = 0
        layer.add(fadeAnimation(start: cue.start, end: cue.end), forKey: "subtitleFade")
        return layer
    }

    private static func speakerLabelLayer(name: String, config: Config) -> CATextLayer {
        let layer = CATextLayer()
        let font = UIFont.systemFont(ofSize: config.fontSize * 0.55, weight: .semibold)
        let attrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: UIColor.white.withAlphaComponent(0.85),
            .kern: 1.5
        ]
        layer.string = NSAttributedString(string: name.uppercased(), attributes: attrs)
        layer.alignmentMode = .center
        layer.contentsScale = UIScreen.main.scale
        // Sit just above the subtitle band; height is a single-line slot.
        let h = config.fontSize * 0.9
        let y = config.bottomInset + (config.fontSize * 4) + 12
        layer.frame = CGRect(
            x: config.horizontalInset,
            y: y,
            width: config.renderSize.width - (config.horizontalInset * 2),
            height: h
        )
        return layer
    }

    private static func subtitleFrame(in config: Config) -> CGRect {
        // Reserve up to four lines worth of vertical space; the text layer
        // word-wraps within. Anchored to the bottom-inset point.
        let height = config.fontSize * 4
        return CGRect(
            x: config.horizontalInset,
            y: config.bottomInset,
            width: config.renderSize.width - (config.horizontalInset * 2),
            height: height
        )
    }

    private static func attributedString(for text: String, config: Config) -> NSAttributedString {
        let font: UIFont
        switch config.style {
        case .editorial:
            font = UIFont.italicSystemFont(ofSize: config.fontSize)
        case .bold:
            font = UIFont.systemFont(ofSize: config.fontSize, weight: .semibold)
        }
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .center
        paragraph.lineBreakMode = .byWordWrapping
        return NSAttributedString(string: text, attributes: [
            .font: font,
            .foregroundColor: UIColor.white,
            .strokeColor: UIColor.black.withAlphaComponent(0.35),
            .strokeWidth: -2.0,
            .paragraphStyle: paragraph
        ])
    }

    // MARK: - Animation

    private static func fadeAnimation(start: TimeInterval, end: TimeInterval) -> CAKeyframeAnimation {
        let total = max(0.001, end - start)
        let fadeIn: TimeInterval = min(0.15, total * 0.2)
        let fadeOut: TimeInterval = min(0.15, total * 0.2)

        let anim = CAKeyframeAnimation(keyPath: "opacity")
        anim.values = [0.0, 1.0, 1.0, 0.0]
        anim.keyTimes = [
            0.0,
            NSNumber(value: fadeIn / total),
            NSNumber(value: 1.0 - (fadeOut / total)),
            1.0
        ]
        anim.beginTime = max(0.0001, start)   // 0 is treated as "now"; nudge to hit start.
        anim.duration = total
        anim.fillMode = .both
        anim.isRemovedOnCompletion = false
        return anim
    }

    // MARK: - Cue derivation

    /// Splits `transcriptText` into rough sentence cues spread evenly across
    /// the clip duration. Used as the v1 fallback when the caller doesn't
    /// have word-level Scribe timings to drive a precise reveal. Sentence
    /// detection is intentionally simple: split on `.`, `!`, `?` plus
    /// trailing whitespace. Aggregates short fragments back together so
    /// "Mr. Smith" doesn't become two cues.
    static func sentenceCues(text: String, duration: TimeInterval) -> [Cue] {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, duration > 0 else { return [] }

        var sentences: [String] = []
        var buffer = ""
        for ch in trimmed {
            buffer.append(ch)
            if ch == "." || ch == "!" || ch == "?" {
                let candidate = buffer.trimmingCharacters(in: .whitespaces)
                // Avoid splitting on "Mr." / single-letter abbreviations: only
                // commit when the buffer has at least 8 chars of content.
                if candidate.count >= 8 {
                    sentences.append(candidate)
                    buffer = ""
                }
            }
        }
        let tail = buffer.trimmingCharacters(in: .whitespaces)
        if !tail.isEmpty { sentences.append(tail) }
        if sentences.isEmpty { sentences = [trimmed] }

        let totalChars = sentences.reduce(0) { $0 + max(1, $1.count) }
        var cursor: TimeInterval = 0
        var cues: [Cue] = []
        cues.reserveCapacity(sentences.count)
        for s in sentences {
            let share = TimeInterval(max(1, s.count)) / TimeInterval(totalChars)
            let span = max(1.0, duration * share)
            let start = cursor
            let end = min(duration, cursor + span)
            cues.append(Cue(text: s, start: start, end: end))
            cursor = end
        }
        return cues
    }
}
