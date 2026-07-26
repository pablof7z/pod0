import Foundation
import Pod0Core

enum LegacyFeedDiscoveryWorkflowMappingError: Error, Equatable {
    case corruptJob(UUID)
    case futurePayload(UUID)
    case duplicateEpisode(UUID)
    case duplicateOccurrence(String)
    case duplicateCandidate(UUID)
}

enum LegacyFeedDiscoveryWorkflowMapper {
    struct Result {
        let candidates: [LegacyFeedDiscoveryCandidateInput]
        let blockedCount: UInt32
    }

    static func map(
        backup: LegacyFeedDiscoveryWorkflowBackup,
        state: AppState,
        now: Date
    ) throws -> Result {
        var episodes: [UUID: Episode] = [:]
        for episode in state.episodes {
            guard episodes.updateValue(episode, forKey: episode.id) == nil else {
                throw LegacyFeedDiscoveryWorkflowMappingError.duplicateEpisode(episode.id)
            }
        }
        let parents = try parentRows(backup.jobs)
        let parentOccurrences = Set(parents.map(\.payload.occurrenceID))
        guard parentOccurrences.count == parents.count else {
            throw LegacyFeedDiscoveryWorkflowMappingError.duplicateOccurrence(
                parents.first?.payload.occurrenceID ?? ""
            )
        }
        var childLinks: [ChildKey: LegacyFeedDiscoveryWorkJob] = [:]
        for job in backup.jobs where job.kind == .newEpisodeNotification {
            guard let parent = linkedParent(job: job, parents: parents) else { continue }
            let key = ChildKey(parent.id, job.subjectID)
            guard childLinks.updateValue(job, forKey: key) == nil else {
                throw LegacyFeedDiscoveryWorkflowMappingError.duplicateCandidate(job.id)
            }
        }
        var candidates: [LegacyFeedDiscoveryCandidateInput] = []
        var blocked: UInt32 = 0
        for parent in parents {
            let mapped = try mapParent(
                parent,
                childLinks: childLinks,
                episodes: episodes,
                artifacts: backup.artifacts,
                now: now
            )
            candidates.append(contentsOf: mapped.candidates)
            blocked = try adding(blocked, mapped.blockedCount)
        }
        for job in backup.jobs where job.kind == .newEpisodeNotification {
            let mapped = try mapChild(
                job,
                parent: linkedParent(job: job, parents: parents),
                episodes: episodes,
                artifacts: backup.artifacts,
                now: now
            )
            if let candidate = mapped.candidate { candidates.append(candidate) }
            blocked = try adding(blocked, mapped.blockedCount)
        }
        let sorted = candidates.sorted(by: candidateOrder)
        let identities = Set(sorted.map {
            "\($0.sourceOccurrenceId.stableString):\($0.episodeId.stableString):\($0.kind)"
        })
        guard identities.count == sorted.count else {
            throw LegacyFeedDiscoveryWorkflowMappingError.duplicateCandidate(
                backup.jobs.first?.id ?? UUID()
            )
        }
        return Result(candidates: sorted, blockedCount: blocked)
    }
}

extension LegacyFeedDiscoveryWorkflowMapper {
    struct Parent {
        let job: LegacyFeedDiscoveryWorkJob
        let payload: LegacyFeedDiscoveryPayload
        var id: UUID { job.id }
    }

    struct ChildKey: Hashable {
        let parentID: UUID
        let episodeID: UUID
        init(_ parentID: UUID, _ episodeID: UUID) {
            self.parentID = parentID
            self.episodeID = episodeID
        }
    }

    struct ParentMapping {
        let candidates: [LegacyFeedDiscoveryCandidateInput]
        let blockedCount: UInt32
    }

    struct ChildMapping {
        let candidate: LegacyFeedDiscoveryCandidateInput?
        let blockedCount: UInt32
    }

    static func parentRows(_ jobs: [LegacyFeedDiscoveryWorkJob]) throws -> [Parent] {
        try jobs.filter { $0.kind == .feedDiscovery }.map { job in
            try validateCommon(job)
            guard let payload = job.payload else {
                throw LegacyFeedDiscoveryWorkflowMappingError.corruptJob(job.id)
            }
            let value: LegacyFeedDiscoveryPayload
            do { value = try decoder.decode(LegacyFeedDiscoveryPayload.self, from: payload) }
            catch { throw LegacyFeedDiscoveryWorkflowMappingError.corruptJob(job.id) }
            guard value.policyVersion == "feed-policy-v1",
                  value.podcastID == job.subjectID,
                  value.episodes.count <= 10_000,
                  Set(value.episodes.map(\.episodeID)).count == value.episodes.count,
                  value.episodes.allSatisfy({ validVersion($0.inputVersion) })
            else { throw LegacyFeedDiscoveryWorkflowMappingError.corruptJob(job.id) }
            return Parent(job: job, payload: value)
        }.sorted { $0.id.uuidString < $1.id.uuidString }
    }

    static func mapParent(
        _ parent: Parent,
        childLinks: [ChildKey: LegacyFeedDiscoveryWorkJob],
        episodes: [UUID: Episode],
        artifacts: [LegacyFeedDiscoveryArtifactRecord],
        now: Date
    ) throws -> ParentMapping {
        let selectedDownloads = selectedDownloadIDs(parent.payload)
        let sortedInputs = parent.payload.episodes.sorted(by: inputOrder)
        var candidates: [LegacyFeedDiscoveryCandidateInput] = []
        var blocked: UInt32 = 0
        for input in sortedInputs {
            guard let episode = episodes[input.episodeID],
                  episode.podcastID == parent.payload.podcastID
            else {
                blocked = try adding(blocked, 1)
                continue
            }
            let current = DesiredStatePlanner.audioVersion(episode) == input.inputVersion
            if selectedDownloads.contains(input.episodeID) {
                candidates.append(candidate(
                    parent: parent,
                    input: input,
                    kind: .download,
                    disposition: parentDisposition(
                        parent.job,
                        kind: .download,
                        current: current,
                        delivered: hasArtifact(
                            .feedDiscovery, job: parent.job, artifacts: artifacts
                        ),
                        expiresAt: parent.payload.discoveredAt.addingTimeInterval(day),
                        now: now
                    )
                ))
            }
            guard parent.payload.notificationsEnabled,
                  sortedInputs.prefix(3).contains(where: { $0.episodeID == input.episodeID }),
                  childLinks[ChildKey(parent.id, input.episodeID)] == nil
            else { continue }
            candidates.append(candidate(
                parent: parent,
                input: input,
                kind: .notification,
                disposition: parentDisposition(
                    parent.job,
                    kind: .notification,
                    current: current,
                    delivered: hasArtifact(.feedDiscovery, job: parent.job, artifacts: artifacts),
                    expiresAt: parent.payload.discoveredAt.addingTimeInterval(day),
                    now: now
                )
            ))
        }
        return ParentMapping(candidates: candidates, blockedCount: blocked)
    }

    static func mapChild(
        _ job: LegacyFeedDiscoveryWorkJob,
        parent: Parent?,
        episodes: [UUID: Episode],
        artifacts: [LegacyFeedDiscoveryArtifactRecord],
        now: Date
    ) throws -> ChildMapping {
        try validateCommon(job)
        guard validVersion(job.inputVersion), let payload = job.payload else {
            throw LegacyFeedDiscoveryWorkflowMappingError.corruptJob(job.id)
        }
        let value: LegacyNewEpisodeNotificationPayload
        do { value = try decoder.decode(LegacyNewEpisodeNotificationPayload.self, from: payload) }
        catch { throw LegacyFeedDiscoveryWorkflowMappingError.corruptJob(job.id) }
        guard let episode = episodes[job.subjectID],
              episode.podcastID == value.podcastID else {
            return ChildMapping(candidate: nil, blockedCount: 1)
        }
        let expires = value.discoveredAt.addingTimeInterval(day)
        let disposition = try childDisposition(
            job,
            current: DesiredStatePlanner.audioVersion(episode) == job.inputVersion,
            delivered: hasArtifact(.notificationDelivery, job: job, artifacts: artifacts),
            expiresAt: expires,
            now: now
        )
        return ChildMapping(
            candidate: LegacyFeedDiscoveryCandidateInput(
                sourceOccurrenceId: CommandId(uuid: parent?.id ?? job.id),
                podcastId: PodcastId(uuid: value.podcastID),
                episodeId: EpisodeId(uuid: job.subjectID),
                kind: .notification,
                disposition: disposition,
                observedAt: UnixTimestampMilliseconds(date: value.discoveredAt),
                expiresAt: UnixTimestampMilliseconds(date: expires),
                publishedAt: UnixTimestampMilliseconds(date: episode.pubDate),
                inputVersion: job.inputVersion
            ),
            blockedCount: 0
        )
    }
}
