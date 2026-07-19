import Foundation
import Pod0Core

enum TranscriptObservationMappingError: Error, Equatable {
    case invalidSourcePayloadDigest
    case invalidTimestamp
}

struct TranscriptObservationContext: Sendable, Equatable {
    let podcastID: UUID
    let sourceRevision: String
    let sourcePayloadDigest: String
    let provider: String?
}

/// Temporary native observation adapter for #96. It is removed or narrowed
/// to a native capability DTO when Rust becomes authoritative in #97.
enum TranscriptObservationMapper {
    static func map(
        _ transcript: Transcript,
        context: TranscriptObservationContext
    ) throws -> TranscriptArtifactInput {
        guard let digest = ContentDigest(hexadecimal: context.sourcePayloadDigest) else {
            throw TranscriptObservationMappingError.invalidSourcePayloadDigest
        }
        let generatedMilliseconds = transcript.generatedAt.timeIntervalSince1970 * 1_000
        guard generatedMilliseconds.isFinite,
              generatedMilliseconds >= Double(Int64.min),
              generatedMilliseconds <= Double(Int64.max)
        else { throw TranscriptObservationMappingError.invalidTimestamp }

        return TranscriptArtifactInput(
            episodeId: EpisodeId(uuid: transcript.episodeID),
            podcastId: PodcastId(uuid: context.podcastID),
            sourceRevision: context.sourceRevision,
            source: coreSource(transcript.source),
            provider: context.provider ?? defaultProvider(for: transcript.source),
            sourcePayloadDigest: digest,
            language: transcript.language,
            generatedAt: UnixTimestampMilliseconds(value: Int64(generatedMilliseconds.rounded())),
            speakers: transcript.speakers.map {
                TranscriptArtifactSpeakerInput(
                    speakerId: SpeakerId(uuid: $0.id),
                    label: $0.label,
                    displayName: $0.displayName
                )
            },
            segments: try transcript.segments.map { segment in
                TranscriptArtifactSegmentInput(
                    text: segment.text,
                    startMilliseconds: try milliseconds(segment.start),
                    endMilliseconds: try milliseconds(segment.end),
                    speakerId: segment.speakerID.map(SpeakerId.init(uuid:)),
                    words: try (segment.words ?? []).map {
                        TranscriptArtifactWordInput(
                            text: $0.text,
                            startMilliseconds: try milliseconds($0.start),
                            endMilliseconds: try milliseconds($0.end)
                        )
                    }
                )
            }
        )
    }

    static func milliseconds(_ seconds: TimeInterval) throws -> UInt64 {
        let value = seconds * 1_000
        guard value.isFinite, value >= 0, value <= Double(UInt64.max) else {
            throw TranscriptObservationMappingError.invalidTimestamp
        }
        return UInt64(value.rounded())
    }

    static func coreSource(_ source: TranscriptSource) -> Pod0Core.TranscriptSource {
        switch source {
        case .publisher: .publisher
        case .scribeV1: .scribe
        case .whisper: .whisper
        case .onDevice: .onDevice
        case .assemblyAI: .assemblyAi
        }
    }

    static func defaultProvider(for source: TranscriptSource) -> String? {
        switch source {
        case .publisher: nil
        case .scribeV1: "elevenLabsScribe"
        case .whisper: "openRouterWhisper"
        case .onDevice: "appleSpeech"
        case .assemblyAI: "assemblyAI"
        }
    }
}
