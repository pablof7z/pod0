import Foundation

// MARK: - Transcript

/// In-app, lossless transcript representation. Same shape regardless of whether
/// the data came from a publisher's `<podcast:transcript>` (Podcasting 2.0
/// JSON, WebVTT, SRT) or from ElevenLabs Scribe / on-device speech analysis.
///
/// Sendable because instances are passed across actor boundaries — UI on
/// `@MainActor`, queue on `TranscriptionQueue`, network on `URLSession`.
struct Transcript: Codable, Sendable, Identifiable, Hashable {
    let id: UUID
    let episodeID: UUID
    let language: String                 // BCP-47, e.g. "en-US"
    let source: TranscriptSource
    let segments: [Segment]
    let speakers: [Speaker]
    let generatedAt: Date

    init(
        id: UUID = UUID(),
        episodeID: UUID,
        language: String,
        source: TranscriptSource,
        segments: [Segment],
        speakers: [Speaker] = [],
        generatedAt: Date = Date()
    ) {
        self.id = id
        self.episodeID = episodeID
        self.language = language
        self.source = source
        self.segments = segments
        self.speakers = speakers
        self.generatedAt = generatedAt
    }
}

// MARK: - Source

/// Where this transcript came from. The brief deliberately collapses the
/// research's ten-source taxonomy into the three that matter for downstream
/// behavior (publisher = trust, Scribe = paid cloud, on-device = private).
enum TranscriptSource: String, Codable, Sendable, Hashable, CaseIterable {
    case publisher
    case scribeV1
    case whisper
    case onDevice
    case assemblyAI
}

// MARK: - Segment

/// A single contiguous span of speech (cue, paragraph, or Scribe segment).
/// `start`/`end` are seconds from the episode origin.
struct Segment: Codable, Sendable, Hashable, Identifiable {
    let id: UUID
    let start: TimeInterval
    let end: TimeInterval
    let speakerID: UUID?
    let text: String
    let words: [Word]?

    init(
        id: UUID = UUID(),
        start: TimeInterval,
        end: TimeInterval,
        speakerID: UUID? = nil,
        text: String,
        words: [Word]? = nil
    ) {
        self.id = id
        self.start = start
        self.end = end
        self.speakerID = speakerID
        self.text = text
        self.words = words
    }
}

// MARK: - Word

/// Word-level timestamp. Optional — only populated when the source supports it
/// (Scribe, Podcasting 2.0 JSON with word arrays). Used for follow-along
/// karaoke highlighting, sentence scrubbing, and clip word-snapping.
struct Word: Codable, Sendable, Hashable {
    let start: TimeInterval
    let end: TimeInterval
    let text: String

    init(start: TimeInterval, end: TimeInterval, text: String) {
        self.start = start
        self.end = end
        self.text = text
    }
}

// MARK: - Speaker

/// One participant in the conversation. `id` is stable within a transcript;
/// `displayName` is resolved later (host detection, show notes, manual edit).
struct Speaker: Codable, Sendable, Hashable, Identifiable {
    let id: UUID
    let label: String                    // raw label from source: "spk_0", "Tim Ferriss", etc.
    let displayName: String?

    init(id: UUID = UUID(), label: String, displayName: String? = nil) {
        self.id = id
        self.label = label
        self.displayName = displayName
    }
}

// MARK: - Convenience

extension Transcript {
    /// Looks up a speaker by id. Used heavily by the reader view.
    func speaker(for id: UUID?) -> Speaker? {
        guard let id else { return nil }
        return speakers.first { $0.id == id }
    }

    /// Returns the segment that contains `time` (or the closest one before it).
    /// O(log n) on a sorted segment list — segments are required to be sorted
    /// by `start` ascending across all parsers.
    func segment(at time: TimeInterval) -> Segment? {
        guard !segments.isEmpty else { return nil }
        var lo = 0
        var hi = segments.count - 1
        var best: Segment?
        while lo <= hi {
            let mid = (lo + hi) / 2
            let s = segments[mid]
            if time < s.start {
                hi = mid - 1
            } else if time > s.end {
                best = s
                lo = mid + 1
            } else {
                return s
            }
        }
        return best
    }
}
