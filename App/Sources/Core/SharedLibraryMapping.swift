import Foundation
import Pod0Core

extension PodcastRecord {
    var swiftValue: Podcast {
        let swiftKind: Podcast.Kind = switch kind {
        case .rss: .rss
        case .synthetic: .synthetic
        case .unsupported: .rss
        }
        return Podcast(
            id: podcastId.uuid!,
            kind: swiftKind,
            feedURL: feedIdentity.flatMap { URL(string: $0.sourceUrl) },
            title: title,
            author: author,
            imageURL: imageUrl.flatMap(URL.init(string:)),
            description: description,
            language: language,
            categories: categories,
            discoveredAt: discoveredAt.date,
            lastRefreshedAt: lastRefreshedAt?.date,
            etag: etag,
            lastModified: lastModified,
            titleIsPlaceholder: titleIsPlaceholder
        )
    }
}

extension PodcastSubscriptionRecord {
    var swiftValue: PodcastSubscription {
        PodcastSubscription(
            podcastID: podcastId.uuid!,
            subscribedAt: subscribedAt.date,
            autoDownload: autoDownload.swiftValue,
            notificationsEnabled: notificationsEnabled,
            defaultPlaybackRate: defaultPlaybackRate.map { Double($0.value) / 1_000 }
        )
    }
}

extension Pod0Core.AutoDownloadPolicy {
    var swiftValue: AutoDownloadPolicy {
        let swiftMode: AutoDownloadPolicy.Mode = switch mode {
        case .off: .off
        case .latest(let count): .latestN(Int(count))
        case .allNew: .allNew
        case .unsupported: .off
        }
        return AutoDownloadPolicy(mode: swiftMode, wifiOnly: wifiOnly)
    }
}

extension AutoDownloadPolicy {
    var coreValue: Pod0Core.AutoDownloadPolicy {
        let coreMode: AutoDownloadMode = switch mode {
        case .off: .off
        case .latestN(let count): .latest(count: UInt16(clamping: count))
        case .allNew: .allNew
        }
        return Pod0Core.AutoDownloadPolicy(mode: coreMode, wifiOnly: wifiOnly)
    }
}

extension EpisodeRecord {
    func swiftValue(preserving adjunct: Episode?) -> Episode? {
        guard let id = episodeId.uuid,
              let podcastID = podcastId.uuid,
              let enclosureURL = URL(string: enclosureUrl)
        else { return nil }
        let completed: Bool = switch listening.completion {
        case .completed: true
        default: false
        }
        return Episode(
            id: id,
            podcastID: podcastID,
            guid: publisherGuid,
            title: title,
            description: description,
            pubDate: publishedAt.date,
            duration: durationMilliseconds.map { Double($0) / 1_000 },
            enclosureURL: enclosureURL,
            enclosureMimeType: enclosureMimeType,
            imageURL: imageUrl.flatMap(URL.init(string:)),
            chapters: adjunct?.chapters,
            persons: mappedPersons(preserving: adjunct?.persons),
            soundBites: mappedSoundBites(preserving: adjunct?.soundBites),
            publisherTranscriptURL: feedMetadata.publisherTranscript.flatMap {
                URL(string: $0.url)
            },
            publisherTranscriptType: feedMetadata.publisherTranscript?.format.swiftValue,
            chaptersURL: feedMetadata.chaptersUrl.flatMap(URL.init(string:)),
            playbackPosition: adjunct?.playbackPosition
                ?? Double(listening.resumePositionMilliseconds) / 1_000,
            played: adjunct?.played ?? completed,
            isStarred: adjunct?.isStarred ?? isStarred,
            downloadState: adjunct?.downloadState ?? .notDownloaded,
            transcriptState: adjunct?.transcriptState ?? .none,
            requestedTranscriptProvider: adjunct?.requestedTranscriptProvider,
            adSegments: adjunct?.adSegments,
            generationSource: adjunct?.generationSource
        )
    }

    private func mappedPersons(preserving existing: [Episode.Person]?) -> [Episode.Person]? {
        guard !feedMetadata.persons.isEmpty else { return nil }
        return feedMetadata.persons.map { person in
            let preserved = existing?.first {
                $0.name == person.name && $0.role == person.role && $0.group == person.group
            }
            return Episode.Person(
                id: preserved?.id ?? UUID(),
                name: person.name,
                role: person.role,
                group: person.group,
                imageURL: person.imageUrl.flatMap(URL.init(string:)),
                linkURL: person.linkUrl.flatMap(URL.init(string:))
            )
        }
    }

    private func mappedSoundBites(
        preserving existing: [Episode.SoundBite]?
    ) -> [Episode.SoundBite]? {
        guard !feedMetadata.soundBites.isEmpty else { return nil }
        return feedMetadata.soundBites.map { soundBite in
            let start = Double(soundBite.startMilliseconds) / 1_000
            let duration = Double(soundBite.durationMilliseconds) / 1_000
            let preserved = existing?.first {
                $0.startTime == start && $0.duration == duration && $0.title == soundBite.title
            }
            return Episode.SoundBite(
                id: preserved?.id ?? UUID(),
                startTime: start,
                duration: duration,
                title: soundBite.title
            )
        }
    }
}

private extension PublisherTranscriptFormat {
    var swiftValue: TranscriptKind? {
        switch self {
        case .json: .json
        case .webVtt: .vtt
        case .subRip: .srt
        case .html: .html
        case .plainText: .text
        case .unknown, .unsupported: nil
        }
    }
}
