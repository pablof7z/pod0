import Foundation
import Pod0Core

extension QueueItem {
    var coreValue: QueueEntry {
        QueueEntry(
            queueEntryId: QueueEntryId(uuid: id),
            episodeId: EpisodeId(uuid: episodeID),
            segment: coreSegment,
            label: label
        )
    }

    private var coreSegment: PlaybackSegment? {
        guard startSeconds != nil || endSeconds != nil else { return nil }
        return PlaybackSegment(
            startPositionMilliseconds: startSeconds.map(Self.milliseconds),
            endPositionMilliseconds: endSeconds.map(Self.milliseconds)
        )
    }

    private static func milliseconds(_ seconds: TimeInterval) -> UInt64 {
        guard seconds.isFinite, seconds > 0 else { return 0 }
        return UInt64(min(seconds * 1_000, Double(UInt64.max)).rounded())
    }
}

extension QueueEntry {
    var swiftValue: QueueItem? {
        guard let id = queueEntryId.uuid, let episodeID = episodeId.uuid else { return nil }
        return QueueItem(
            id: id,
            episodeID: episodeID,
            startSeconds: segment?.startPositionMilliseconds.map(Self.seconds),
            endSeconds: segment?.endPositionMilliseconds.map(Self.seconds),
            label: label
        )
    }

    private static func seconds(_ milliseconds: UInt64) -> TimeInterval {
        Double(milliseconds) / 1_000
    }
}

extension PlaybackSleepTimer {
    var coreValue: PlaybackSleepMode {
        switch self {
        case .off:
            .off
        case .minutes(let minutes):
            .duration(durationMilliseconds: UInt64(max(0, minutes)) * 60_000)
        case .endOfEpisode:
            .endOfEpisode
        }
    }
}

extension PlaybackSleepMode {
    var swiftValue: PlaybackSleepTimer {
        switch self {
        case .off, .unsupported:
            .off
        case .duration(let durationMilliseconds):
            .minutes(max(1, Int((durationMilliseconds + 59_999) / 60_000)))
        case .endOfEpisode:
            .endOfEpisode
        }
    }
}
