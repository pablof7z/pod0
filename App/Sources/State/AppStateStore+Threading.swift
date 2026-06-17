import Foundation

// MARK: - Rust-owned threading projection

struct ThreadingProjection: Equatable, Sendable {
    var topics: [ThreadingTopic]
    var mentions: [ThreadingMention]

    static let empty = ThreadingProjection(topics: [], mentions: [])
}

struct ActiveThreadingTopic: Equatable, Identifiable, Sendable {
    let topic: ThreadingTopic
    let unplayedEpisodeCount: Int
    let mentionIDs: [UUID]
    var id: UUID { topic.id }
}

extension AppStateStore {

    var threadingTopics: [ThreadingTopic] {
        threadingProjection.topics
    }

    func refreshThreadingProjection() {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let envelope = kernel?.threadingProjectionEnvelope(),
              let data = envelope.data(using: .utf8),
              let decoded = try? decoder.decode(ThreadingProjectionEnvelope.self, from: data)
        else { return }
        threadingProjection = decoded.projection
    }

    func threadingTopic(id: UUID) -> ThreadingTopic? {
        threadingProjection.topics.first { $0.id == id }
    }

    func threadingMentions(forTopic id: UUID) -> [ThreadingMention] {
        threadingProjection.mentions.filter { $0.topicID == id }
    }

    func threadingMentions(containingEpisode episodeID: UUID) -> [ThreadingMention] {
        threadingProjection.mentions.filter { $0.episodeID == episodeID }
    }

    func activeThreadingTopics(
        limit: Int,
        subscriptionFilter: Set<UUID>? = nil
    ) -> [ActiveThreadingTopic] {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let podcastIDs = subscriptionFilter.map { Array($0) } ?? []
        guard let envelope = kernel?.threadingActiveTopicsEnvelope(limit: limit, podcastIDs: podcastIDs),
              let data = envelope.data(using: .utf8),
              let decoded = try? decoder.decode(ActiveThreadingTopicsEnvelope.self, from: data)
        else { return [] }
        return decoded.activeTopics.compactMap { row in
            guard let topicID = UUID(uuidString: row.topicId),
                  let topic = threadingTopic(id: topicID) else { return nil }
            let mentionIDs = row.mentionIds.compactMap(UUID.init(uuidString:))
            return ActiveThreadingTopic(
                topic: topic,
                unplayedEpisodeCount: row.unplayedEpisodeCount,
                mentionIDs: mentionIDs
            )
        }
    }
}

private struct ThreadingProjectionEnvelope: Decodable {
    var topics: [ThreadingTopicDTO] = []
    var mentions: [ThreadingMentionDTO] = []

    var projection: ThreadingProjection {
        ThreadingProjection(
            topics: topics.compactMap(\.topic),
            mentions: mentions.compactMap(\.mention)
        )
    }
}

private struct ActiveThreadingTopicsEnvelope: Decodable {
    var activeTopics: [ActiveThreadingTopicDTO] = []
}

private struct ActiveThreadingTopicDTO: Decodable {
    var topicId: String
    var unplayedEpisodeCount: Int
    var mentionIds: [String]
}

private struct ThreadingTopicDTO: Decodable {
    var id: String
    var slug: String
    var displayName: String
    var definition: String?
    var episodeMentionCount: Int
    var contradictionCount: Int
    var lastMentionedAt: Int?

    var topic: ThreadingTopic? {
        guard let uuid = UUID(uuidString: id) else { return nil }
        return ThreadingTopic(
            id: uuid,
            slug: slug,
            displayName: displayName,
            definition: definition,
            episodeMentionCount: episodeMentionCount,
            contradictionCount: contradictionCount,
            lastMentionedAt: lastMentionedAt.map { Date(timeIntervalSince1970: TimeInterval($0)) }
        )
    }
}

private struct ThreadingMentionDTO: Decodable {
    var id: String
    var topicId: String
    var episodeId: String
    var startMs: Int
    var endMs: Int
    var snippet: String
    var confidence: Double
    var isContradictory: Bool

    var mention: ThreadingMention? {
        guard let id = UUID(uuidString: id),
              let topicID = UUID(uuidString: topicId),
              let episodeID = UUID(uuidString: episodeId) else { return nil }
        return ThreadingMention(
            id: id,
            topicID: topicID,
            episodeID: episodeID,
            startMS: startMs,
            endMS: endMs,
            snippet: snippet,
            confidence: confidence,
            isContradictory: isContradictory
        )
    }
}
