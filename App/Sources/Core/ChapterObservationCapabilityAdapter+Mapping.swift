import CryptoKit
import Foundation
import Pod0Core

extension ChapterObservationCapabilityAdapter {
    static func preflight(
        _ request: ChapterCapabilityRequest,
        limits: ChapterObservationLimits
    ) -> ChapterCapabilityFailure? {
        switch request {
        case .model(let value):
            let systemBytes = UInt64(value.planned.systemPrompt.utf8.count)
            let userBytes = UInt64(value.planned.userPrompt.utf8.count)
            let total = systemBytes.addingReportingOverflow(userBytes)
            guard value.planned.maximumCompletionBytes > 0,
                  value.planned.maximumCompletionBytes <= limits.modelCompletionBytes,
                  value.planned.responseFormat == .jsonObject
            else { return .invalidRequest("Invalid shared chapter model policy") }
            guard !total.overflow,
                  total.partialValue <= limits.modelPromptBytes
            else {
                return .responseTooLarge("Chapter model prompt exceeds core limit")
            }
        case .agent(let value):
            guard value.items.count <= Int(limits.agentItems) else {
                return .responseTooLarge("Agent chapter items exceed core limit")
            }
        }
        return nil
    }

    func qualifyModel(
        _ response: ChapterModelTransportResponse,
        request: ModelChapterCapabilityRequest,
        limits: ChapterObservationLimits
    ) -> ChapterCapabilityOutcome {
        let byteCount = UInt64(response.completion.utf8.count)
        guard byteCount <= limits.modelCompletionBytes else {
            return .failed(.responseTooLarge("Chapter model completion exceeds core limit"))
        }
        guard !response.provider.isEmpty,
              response.provider.trimmed == response.provider,
              UInt64(response.provider.utf8.count) <= limits.providerBytes,
              !response.model.isEmpty,
              response.model.trimmed == response.model,
              UInt64(response.model.utf8.count) <= limits.modelBytes
        else {
            return .failed(.invalidMetadata("Malformed chapter model metadata"))
        }
        let digest = Self.digest(Data(response.completion.utf8))
        let planned = request.planned
        let observation = ModelChapterObservation(
            episodeId: planned.episodeId,
            podcastId: planned.podcastId,
            formatVersion: planned.formatVersion,
            requestedTranscriptVersionId: planned.requestedTranscriptVersionId,
            requestedTranscriptContentDigest: planned.requestedTranscriptContentDigest,
            selectedTranscriptVersionId: planned.selectedTranscriptVersionId,
            selectedTranscriptContentDigest: planned.selectedTranscriptContentDigest,
            policyVersion: planned.policyVersion,
            sourceVersion: planned.sourceVersion,
            provider: response.provider,
            model: response.model,
            completionDigest: digest,
            completion: response.completion,
            generatedAt: request.generatedAt,
            durationMilliseconds: planned.durationMilliseconds,
            mode: planned.mode
        )
        return qualify(
            .model(observation),
            evidence: .model(ChapterModelEvidence(
                provider: response.provider,
                model: response.model,
                usage: response.usage,
                completionDigest: digest,
                completionByteCount: byteCount
            ))
        )
    }

    private static func digest(_ data: Data) -> ContentDigest {
        let bytes = Array(SHA256.hash(data: data))
        func word(_ offset: Int) -> UInt64 {
            bytes[offset..<(offset + 8)].reduce(0) { ($0 << 8) | UInt64($1) }
        }
        return ContentDigest(
            word0: word(0),
            word1: word(8),
            word2: word(16),
            word3: word(24)
        )
    }
}
