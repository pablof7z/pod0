import Foundation
import Pod0Core

extension LegacyFeedDiscoveryWorkflowMapper {
    static let day: TimeInterval = 24 * 60 * 60

    static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()

    static func validateCommon(_ job: LegacyFeedDiscoveryWorkJob) throws {
        guard job.payloadVersion == 1 else {
            throw LegacyFeedDiscoveryWorkflowMappingError.futurePayload(job.id)
        }
    }

    static func linkedParent(
        job: LegacyFeedDiscoveryWorkJob,
        parents: [Parent]
    ) -> Parent? {
        parents.first {
            job.occurrenceID
                == "notification:\($0.payload.occurrenceID):\(job.subjectID.uuidString)"
        }
    }

    static func selectedDownloadIDs(_ payload: LegacyFeedDiscoveryPayload) -> Set<UUID> {
        let ordered = payload.episodes.sorted(by: inputOrder)
        let selected: ArraySlice<LegacyFeedDiscoveryPayload.EpisodeInput>
        switch payload.autoDownloadPolicy?.mode {
        case .latestN(let count):
            selected = ordered.prefix(max(0, count))
        case .allNew:
            selected = ordered[...]
        case .off, .none:
            selected = []
        }
        return Set(selected.map(\.episodeID))
    }

    static func parentDisposition(
        _ job: LegacyFeedDiscoveryWorkJob,
        kind: LegacyFeedDiscoveryEffectKindInput,
        current: Bool,
        delivered: Bool,
        expiresAt: Date,
        now: Date
    ) -> LegacyFeedDiscoveryDispositionInput {
        if delivered || job.state == .succeeded { return .succeeded(attempt: 0) }
        if !current || now >= expiresAt { return .obsolete(attempt: 0) }
        switch job.state {
        case .leased where kind == .notification,
             .running where kind == .notification:
            return .ambiguous(attempt: 0)
        case .pending, .leased, .running, .retryScheduled, .blocked:
            return .pending(attempt: 0, notBefore: nil)
        case .failedPermanent:
            return .failed(attempt: 0)
        case .cancelled, .obsolete:
            return .obsolete(attempt: 0)
        case .succeeded:
            return .succeeded(attempt: 0)
        }
    }

    static func childDisposition(
        _ job: LegacyFeedDiscoveryWorkJob,
        current: Bool,
        delivered: Bool,
        expiresAt: Date,
        now: Date
    ) throws -> LegacyFeedDiscoveryDispositionInput {
        guard (0...4).contains(job.attempt) else {
            throw LegacyFeedDiscoveryWorkflowMappingError.corruptJob(job.id)
        }
        let attempt = UInt8(job.attempt)
        if delivered || job.state == .succeeded { return .succeeded(attempt: attempt) }
        if !current || now >= expiresAt { return .obsolete(attempt: attempt) }
        if job.state == .leased || job.state == .running || job.leaseToken != nil
            || job.externalOperationID != nil || job.externalOperationState != nil {
            return .ambiguous(attempt: attempt)
        }
        switch job.state {
        case .pending, .retryScheduled, .blocked:
            return .pending(
                attempt: attempt,
                notBefore: UnixTimestampMilliseconds(date: job.notBefore)
            )
        case .failedPermanent:
            return .failed(attempt: attempt)
        case .cancelled, .obsolete:
            return .obsolete(attempt: attempt)
        case .leased, .running:
            return .ambiguous(attempt: attempt)
        case .succeeded:
            return .succeeded(attempt: attempt)
        }
    }

    static func candidate(
        parent: Parent,
        input: LegacyFeedDiscoveryPayload.EpisodeInput,
        kind: LegacyFeedDiscoveryEffectKindInput,
        disposition: LegacyFeedDiscoveryDispositionInput
    ) -> LegacyFeedDiscoveryCandidateInput {
        let expires = parent.payload.discoveredAt.addingTimeInterval(day)
        return LegacyFeedDiscoveryCandidateInput(
            sourceOccurrenceId: CommandId(uuid: parent.id),
            podcastId: PodcastId(uuid: parent.payload.podcastID),
            episodeId: EpisodeId(uuid: input.episodeID),
            kind: kind,
            disposition: disposition,
            observedAt: UnixTimestampMilliseconds(date: parent.payload.discoveredAt),
            expiresAt: UnixTimestampMilliseconds(date: expires),
            publishedAt: UnixTimestampMilliseconds(date: input.pubDate),
            inputVersion: input.inputVersion
        )
    }

    static func hasArtifact(
        _ kind: LegacyFeedDiscoveryArtifactKind,
        job: LegacyFeedDiscoveryWorkJob,
        artifacts: [LegacyFeedDiscoveryArtifactRecord]
    ) -> Bool {
        artifacts.contains {
            $0.kind == kind && $0.subjectID == job.subjectID
                && $0.inputVersion == job.inputVersion && $0.integrity == .available
        }
    }

    static func validVersion(_ value: String) -> Bool {
        value.count == 64 && value.utf8.allSatisfy {
            (48...57).contains($0) || (97...102).contains($0)
        }
    }

    static func inputOrder(
        _ lhs: LegacyFeedDiscoveryPayload.EpisodeInput,
        _ rhs: LegacyFeedDiscoveryPayload.EpisodeInput
    ) -> Bool {
        if lhs.pubDate != rhs.pubDate { return lhs.pubDate > rhs.pubDate }
        return lhs.episodeID.uuidString < rhs.episodeID.uuidString
    }

    static func candidateOrder(
        _ lhs: LegacyFeedDiscoveryCandidateInput,
        _ rhs: LegacyFeedDiscoveryCandidateInput
    ) -> Bool {
        if lhs.sourceOccurrenceId != rhs.sourceOccurrenceId {
            return lhs.sourceOccurrenceId.stableString < rhs.sourceOccurrenceId.stableString
        }
        if lhs.episodeId != rhs.episodeId {
            return lhs.episodeId.stableString < rhs.episodeId.stableString
        }
        return String(describing: lhs.kind) < String(describing: rhs.kind)
    }

    static func adding(_ lhs: UInt32, _ rhs: UInt32) throws -> UInt32 {
        let result = lhs.addingReportingOverflow(rhs)
        guard !result.overflow else {
            throw LegacyFeedDiscoveryWorkflowMappingError.corruptJob(UUID())
        }
        return result.partialValue
    }
}
