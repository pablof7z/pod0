import Foundation
import Pod0Core

enum SharedClipMappingError: Error, Equatable {
    case invalidBounds
    case invalidSpeaker
}

extension ClipRecord {
    var swiftValue: Clip? {
        guard let id = clipId.uuid,
              let episodeID = episodeId.uuid,
              let podcastID = podcastId.uuid,
              revision.value > 0,
              let start = Int(exactly: startMilliseconds),
              let end = Int(exactly: endMilliseconds),
              start < end,
              let source = source.swiftValue
        else { return nil }
        let speaker: String?
        switch (speakerId, speakerLabel) {
        case (nil, nil):
            speaker = nil
        case (.some(let value), nil):
            guard let uuid = value.uuid else { return nil }
            speaker = uuid.uuidString
        case (nil, .some(let label)):
            speaker = label
        case (.some, .some):
            return nil
        }
        return Clip(
            id: id,
            revision: revision.value,
            episodeID: episodeID,
            subscriptionID: podcastID,
            startMs: start,
            endMs: end,
            createdAt: createdAt.date,
            caption: caption,
            speakerID: speaker,
            transcriptText: frozenTranscriptText,
            source: source,
            deleted: deleted,
            evidence: evidence?.swiftValue
        )
    }
}

extension Clip.Source {
    var coreValue: Pod0Core.ClipSource {
        switch self {
        case .touch: .touch
        case .auto: .auto
        case .headphone: .headphone
        case .carplay: .carplay
        case .watch: .watch
        case .siri: .siri
        case .agent: .agent
        case .unsupported: .unsupported(wireCode: 0)
        }
    }
}

extension Clip {
    var coreStartMilliseconds: UInt64? {
        UInt64(exactly: startMs)
    }

    var coreEndMilliseconds: UInt64? {
        UInt64(exactly: endMs)
    }

    func coreSpeakerID(preservingLegacyLabel: Bool = false) throws -> SpeakerId? {
        guard let speakerID else { return nil }
        guard let uuid = UUID(uuidString: speakerID) else {
            if preservingLegacyLabel { return nil }
            throw SharedClipMappingError.invalidSpeaker
        }
        return SpeakerId(uuid: uuid)
    }
}

private extension Pod0Core.ClipSource {
    var swiftValue: Clip.Source? {
        switch self {
        case .touch: .touch
        case .auto: .auto
        case .headphone: .headphone
        case .carplay: .carplay
        case .watch: .watch
        case .siri: .siri
        case .agent: .agent
        case .unsupported: .unsupported
        }
    }
}

private extension ClipEvidenceReference {
    var swiftValue: ClipEvidence {
        ClipEvidence(
            generationID: generationId.stableString,
            transcriptVersionID: transcriptVersionId.stableString,
            transcriptContentDigest: transcriptContentDigest.stableString,
            spanID: spanId.stableString
        )
    }
}
