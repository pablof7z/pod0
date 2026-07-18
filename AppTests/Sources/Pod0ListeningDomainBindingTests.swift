import Foundation
import Pod0Core
@testable import Podcastr
import XCTest

final class Pod0ListeningDomainBindingTests: XCTestCase {
    func testCurrentAndLegacySwiftRecordsRoundTripThroughRust() throws {
        let fixture = try loadListeningFixture()
        let podcastUUID = try XCTUnwrap(UUID(uuidString: fixture["podcast_uuid"]!))
        let episodeUUID = try XCTUnwrap(UUID(uuidString: fixture["episode_uuid"]!))
        let feedURL = try XCTUnwrap(URL(string: fixture["feed_source_url"]!))
        let podcast = Podcast(
            id: podcastUUID,
            kind: .rss,
            feedURL: feedURL,
            title: fixture["podcast_title"]!,
            discoveredAt: date(fixture, "podcast_discovered_at_ms")
        )
        let subscription = PodcastSubscription(
            podcastID: podcastUUID,
            subscribedAt: date(fixture, "subscription_subscribed_at_ms"),
            autoDownload: .init(mode: .latestN(int(fixture, "auto_download_latest_count")), wifiOnly: true),
            notificationsEnabled: true,
            defaultPlaybackRate: doublePermille(fixture, "default_playback_rate_permille")
        )
        let episode = Episode(
            id: episodeUUID,
            podcastID: podcastUUID,
            guid: fixture["episode_guid"]!,
            title: fixture["episode_title"]!,
            pubDate: date(fixture, "episode_published_at_ms"),
            duration: seconds(fixture, "episode_duration_ms"),
            enclosureURL: try XCTUnwrap(URL(string: fixture["episode_enclosure_url"]!)),
            enclosureMimeType: fixture["episode_enclosure_mime"],
            playbackPosition: seconds(fixture, "episode_resume_position_ms"),
            downloadState: .downloaded(
                localFileURL: URL(fileURLWithPath: "/fixture/episode-42.mp3"),
                byteCount: Int64(fixture["download_byte_count"]!)!
            ),
            transcriptState: .ready(source: .publisher)
        )

        let currentPodcast: Podcast = try codableRoundTrip(podcast)
        let currentSubscription: PodcastSubscription = try codableRoundTrip(subscription)
        let currentEpisode: Episode = try codableRoundTrip(episode)
        let legacyPodcast: Podcast = try legacyRoundTrip(
            podcast,
            replacing: nil,
            removing: ["kind", "categories", "titleIsPlaceholder"]
        )
        let legacySubscription: PodcastSubscription = try legacyRoundTrip(
            subscription,
            replacing: ("podcastID", fixture["subscription_legacy_id_key"]!),
            removing: []
        )
        let legacyEpisode: Episode = try legacyRoundTrip(
            episode,
            replacing: ("podcastID", fixture["episode_legacy_parent_key"]!),
            removing: []
        )

        XCTAssertEqual(currentPodcast.id, legacyPodcast.id)
        XCTAssertEqual(currentSubscription.podcastID, legacySubscription.podcastID)
        XCTAssertEqual(currentEpisode.podcastID, legacyEpisode.podcastID)
        XCTAssertEqual(currentEpisode.playbackPosition, legacyEpisode.playbackPosition)
        XCTAssertEqual(fixture["completion_percentage_threshold"], "none")

        let snapshot = try coreSnapshot(
            fixture: fixture,
            podcast: legacyPodcast,
            subscription: legacySubscription,
            episode: legacyEpisode
        )
        XCTAssertEqual(try validateListeningSnapshot(snapshot: snapshot), snapshot)
        XCTAssertEqual(snapshot.playback.queue.count, 2)
        XCTAssertEqual(snapshot.playback.queue.map(\.episodeId), [snapshot.episodes[0].episodeId, snapshot.episodes[0].episodeId])
        XCTAssertNotEqual(
            snapshot.playback.queue[0].queueEntryId,
            snapshot.playback.queue[1].queueEntryId
        )
    }

    func testCoreIdentityRulesPreserveExistingIdsAndLegacyPrecedence() throws {
        let fixture = try loadListeningFixture()
        let existing = PodcastId(
            high: uint(fixture, "podcast_id_high"),
            low: uint(fixture, "podcast_id_low")
        )
        let incoming = PodcastId(
            high: uint(fixture, "incoming_podcast_id_high"),
            low: uint(fixture, "incoming_podcast_id_low")
        )
        let feed = try makeFeedIdentityV1(feedUrl: fixture["feed_source_url"]!)
        let resolution = try resolvePodcastIdentityV1(
            incomingId: incoming,
            incomingFeedUrl: fixture["feed_source_url"]!,
            existing: [PodcastIdentityRecord(podcastId: existing, feedIdentity: feed)]
        )

        XCTAssertEqual(resolution, .preserveExisting(podcastId: existing))
        XCTAssertEqual(feed.comparisonKey, fixture["feed_comparison_key"])
        XCTAssertEqual(
            try resolveLegacyParentId(modernParentId: existing, legacyParentId: incoming),
            existing
        )
        XCTAssertEqual(
            try resolveLegacyParentId(modernParentId: nil, legacyParentId: existing),
            existing
        )

        let existingEpisode = EpisodeId(
            high: uint(fixture, "episode_id_high"),
            low: uint(fixture, "episode_id_low")
        )
        let incomingEpisode = EpisodeId(
            high: uint(fixture, "incoming_episode_id_high"),
            low: uint(fixture, "incoming_episode_id_low")
        )
        XCTAssertEqual(
            try resolveEpisodeIdentityV1(
                incomingId: incomingEpisode,
                podcastId: existing,
                publisherGuid: fixture["episode_guid"]!,
                existing: [EpisodeIdentityRecord(
                    episodeId: existingEpisode,
                    podcastId: existing,
                    publisherGuid: fixture["episode_guid"]!
                )]
            ),
            .preserveExisting(episodeId: existingEpisode)
        )
    }

    private func coreSnapshot(
        fixture: [String: String],
        podcast: Podcast,
        subscription: PodcastSubscription,
        episode: Episode
    ) throws -> ListeningDomainSnapshot {
        let podcastID = corePodcastID(podcast.id)
        let episodeID = coreEpisodeID(episode.id)
        let reference = { (versionKey: String, opaqueKey: String) in
            ArtifactReference(
                schemaVersion: UInt32(fixture[versionKey]!)!,
                opaqueKey: fixture[opaqueKey]!
            )
        }
        let queueID = { (prefix: String) in
            QueueEntryId(
                high: self.uint(fixture, "\(prefix)_high"),
                low: self.uint(fixture, "\(prefix)_low")
            )
        }
        return ListeningDomainSnapshot(
            podcasts: [PodcastRecord(
                podcastId: podcastID,
                kind: .rss,
                feedIdentity: try makeFeedIdentityV1(feedUrl: podcast.feedURL!.absoluteString),
                title: podcast.title,
                discoveredAt: .init(value: milliseconds(podcast.discoveredAt))
            )],
            subscriptions: [PodcastSubscriptionRecord(
                podcastId: podcastID,
                subscribedAt: .init(value: milliseconds(subscription.subscribedAt)),
                autoDownload: Pod0Core.AutoDownloadPolicy(
                    mode: .latest(count: UInt16(int(fixture, "auto_download_latest_count"))),
                    wifiOnly: subscription.autoDownload.wifiOnly
                ),
                notificationsEnabled: subscription.notificationsEnabled,
                defaultPlaybackRate: .init(value: UInt16(fixture["default_playback_rate_permille"]!)!)
            )],
            episodes: [EpisodeRecord(
                episodeId: episodeID,
                podcastId: podcastID,
                publisherGuid: episode.guid,
                title: episode.title,
                publishedAt: .init(value: milliseconds(episode.pubDate)),
                durationMilliseconds: UInt64(milliseconds(episode.duration!)),
                enclosureUrl: episode.enclosureURL.absoluteString,
                enclosureMimeType: episode.enclosureMimeType,
                listening: EpisodeListeningState(
                    resumePositionMilliseconds: UInt64(milliseconds(episode.playbackPosition)),
                    completion: .inProgress
                ),
                download: .available(
                    reference: reference("download_schema_version", "download_opaque_key"),
                    byteCount: uint(fixture, "download_byte_count")
                ),
                transcript: .available(
                    reference: reference("transcript_schema_version", "transcript_opaque_key"),
                    source: .publisher
                )
            )],
            playback: ListeningPlaybackPolicy(
                activeEpisodeId: episodeID,
                queue: [
                    Pod0Core.QueueEntry(
                        queueEntryId: queueID("queue_whole_id"),
                        episodeId: episodeID,
                        segment: nil,
                        label: nil
                    ),
                    Pod0Core.QueueEntry(
                        queueEntryId: queueID("queue_segment_id"),
                        episodeId: episodeID,
                        segment: PlaybackSegment(
                            startPositionMilliseconds: uint(fixture, "queue_segment_start_ms"),
                            endPositionMilliseconds: uint(fixture, "queue_segment_end_ms")
                        ),
                        label: fixture["queue_segment_label"]
                    ),
                ],
                rate: .init(value: UInt16(fixture["playback_rate_permille"]!)!),
                sleepMode: .duration(durationMilliseconds: uint(fixture, "sleep_duration_ms")),
                autoMarkPlayedAtNaturalEnd: true,
                autoPlayNext: true,
                revision: .init(value: uint(fixture, "state_revision"))
            )
        )
    }

    private func loadListeningFixture() throws -> [String: String] {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(
            forResource: "listening-domain-v1",
            withExtension: "properties"
        ))
        return try String(contentsOf: url, encoding: .utf8)
            .split(whereSeparator: \.isNewline)
            .filter { !$0.isEmpty && !$0.hasPrefix("#") }
            .reduce(into: [:]) { output, line in
                let parts = line.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
                if parts.count == 2 { output[String(parts[0])] = String(parts[1]) }
            }
    }

    private func codableRoundTrip<T: Codable>(_ value: T) throws -> T {
        try JSONDecoder().decode(T.self, from: JSONEncoder().encode(value))
    }

    private func legacyRoundTrip<T: Codable>(
        _ value: T,
        replacing keys: (String, String)?,
        removing: [String]
    ) throws -> T {
        var object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: JSONEncoder().encode(value)) as? [String: Any]
        )
        if let keys { object[keys.1] = object.removeValue(forKey: keys.0) }
        removing.forEach { object.removeValue(forKey: $0) }
        object["futureFieldUnknownToV1"] = "ignored"
        return try JSONDecoder().decode(T.self, from: JSONSerialization.data(withJSONObject: object))
    }

    private func corePodcastID(_ id: UUID) -> PodcastId {
        let parts = uuidParts(id)
        return PodcastId(high: parts.high, low: parts.low)
    }

    private func coreEpisodeID(_ id: UUID) -> EpisodeId {
        let parts = uuidParts(id)
        return EpisodeId(high: parts.high, low: parts.low)
    }

    private func uuidParts(_ id: UUID) -> (high: UInt64, low: UInt64) {
        let hex = id.uuidString.replacingOccurrences(of: "-", with: "")
        return (
            UInt64(hex.prefix(16), radix: 16)!,
            UInt64(hex.suffix(16), radix: 16)!
        )
    }

    private func uint(_ values: [String: String], _ key: String) -> UInt64 { UInt64(values[key]!)! }
    private func int(_ values: [String: String], _ key: String) -> Int { Int(values[key]!)! }
    private func date(_ values: [String: String], _ key: String) -> Date {
        Date(timeIntervalSince1970: Double(values[key]!)! / 1_000)
    }
    private func seconds(_ values: [String: String], _ key: String) -> Double {
        Double(values[key]!)! / 1_000
    }
    private func doublePermille(_ values: [String: String], _ key: String) -> Double {
        Double(values[key]!)! / 1_000
    }
    private func milliseconds(_ date: Date) -> Int64 { Int64((date.timeIntervalSince1970 * 1_000).rounded()) }
    private func milliseconds(_ seconds: Double) -> Int64 { Int64((seconds * 1_000).rounded()) }
}
