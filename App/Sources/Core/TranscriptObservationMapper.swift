import Foundation
import Pod0Core

enum TranscriptObservationMappingError: Error, Equatable {
    case invalidSourcePayloadDigest
    case invalidSpeakerIdentity
    case unknownSpeakerReference
    case invalidTimestamp
}

struct TranscriptObservationContext: Sendable, Equatable {
    let podcastID: UUID
    let sourceRevision: String
    let sourcePayloadDigest: String
    let provider: String?
}

/// Maps raw native/provider observations into the typed Rust command contract.
/// It owns no durable state or transcript-selection policy.
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
        let speakerIDs = try stableSpeakerIDs(
            transcript.speakers,
            episodeID: transcript.episodeID,
            sourceRevision: context.sourceRevision
        )
        let segments = try transcript.segments.map { segment in
            let speakerID: SpeakerId?
            if let nativeID = segment.speakerID {
                guard let stableID = speakerIDs[nativeID] else {
                    throw TranscriptObservationMappingError.unknownSpeakerReference
                }
                speakerID = stableID
            } else {
                speakerID = nil
            }
            return TranscriptArtifactSegmentInput(
                text: segment.text,
                startMilliseconds: try milliseconds(segment.start),
                endMilliseconds: try milliseconds(segment.end),
                speakerId: speakerID,
                words: try (segment.words ?? []).map {
                    TranscriptArtifactWordInput(
                        text: $0.text,
                        startMilliseconds: try milliseconds($0.start),
                        endMilliseconds: try milliseconds($0.end)
                    )
                }
            )
        }

        return TranscriptArtifactInput(
            episodeId: EpisodeId(uuid: transcript.episodeID),
            podcastId: PodcastId(uuid: context.podcastID),
            sourceRevision: context.sourceRevision,
            source: coreSource(transcript.source),
            provider: context.provider ?? defaultProvider(for: transcript.source),
            sourcePayloadDigest: digest,
            language: transcript.language,
            generatedAt: UnixTimestampMilliseconds(value: Int64(generatedMilliseconds.rounded())),
            speakers: try transcript.speakers.map {
                guard let speakerID = speakerIDs[$0.id] else {
                    throw TranscriptObservationMappingError.invalidSpeakerIdentity
                }
                return TranscriptArtifactSpeakerInput(
                    speakerId: speakerID,
                    label: $0.label,
                    displayName: $0.displayName
                )
            },
            segments: segments
        )
    }

    private static func stableSpeakerIDs(
        _ speakers: [Speaker],
        episodeID: UUID,
        sourceRevision: String
    ) throws -> [UUID: SpeakerId] {
        var nativeIDs = Set<UUID>()
        var labels = Set<String>()
        var result: [UUID: SpeakerId] = [:]
        for speaker in speakers {
            guard nativeIDs.insert(speaker.id).inserted,
                  labels.insert(speaker.label).inserted,
                  let stableID = transcriptSpeakerId(
                    episodeId: EpisodeId(uuid: episodeID),
                    sourceRevision: sourceRevision,
                    label: speaker.label
                  )
            else { throw TranscriptObservationMappingError.invalidSpeakerIdentity }
            result[speaker.id] = stableID
        }
        return result
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
