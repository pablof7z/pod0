import Foundation
import Pod0Core

enum SharedTranscriptReadError: Error, Equatable {
    case unavailable
    case unsupportedSource
    case invalidProjection
}

struct SharedTranscriptSegment: Sendable, Equatable {
    let coreID: TranscriptSegmentId
    let ordinal: UInt32
    let value: Segment
    let wordCount: UInt32
}

/// Native projection adapter over the Rust-authoritative transcript store.
/// `load` reconstructs the existing presentation model from bounded pages.
final class SharedTranscriptReader: TranscriptReading, @unchecked Sendable {
    static let maximumPageSize: UInt16 = 200

    private let facade: Pod0Facade

    init(facade: Pod0Facade) {
        self.facade = facade
    }

    func summary(episodeID: UUID) throws -> TranscriptSummaryProjection? {
        try page(episodeID: episodeID, scope: .summary, offset: 0, maxItems: 1).summary
    }

    func speakersPage(
        episodeID: UUID,
        offset: UInt32,
        maxItems: UInt16 = maximumPageSize
    ) throws -> (items: [Speaker], hasMore: Bool) {
        let projection = try page(
            episodeID: episodeID,
            scope: .speakers,
            offset: offset,
            maxItems: maxItems
        )
        let values = try projection.speakers.map { value -> Speaker in
            guard let id = value.speakerId.uuid else {
                throw SharedTranscriptReadError.invalidProjection
            }
            return Speaker(id: id, label: value.label, displayName: value.displayName)
        }
        return (values, projection.hasMore)
    }

    func segmentsPage(
        episodeID: UUID,
        offset: UInt32,
        maxItems: UInt16 = maximumPageSize
    ) throws -> (items: [SharedTranscriptSegment], hasMore: Bool) {
        let projection = try page(
            episodeID: episodeID,
            scope: .segments,
            offset: offset,
            maxItems: maxItems
        )
        return (try projection.segments.map(segment), projection.hasMore)
    }

    func exactSegment(
        episodeID: UUID,
        segmentID: TranscriptSegmentId
    ) throws -> SharedTranscriptSegment? {
        try page(
            episodeID: episodeID,
            scope: .segment(segmentId: segmentID),
            offset: 0,
            maxItems: 1
        ).segments.first.map(segment)
    }

    func wordsPage(
        episodeID: UUID,
        segmentID: TranscriptSegmentId,
        offset: UInt32,
        maxItems: UInt16 = maximumPageSize
    ) throws -> (items: [Word], hasMore: Bool) {
        let projection = try page(
            episodeID: episodeID,
            scope: .words(segmentId: segmentID),
            offset: offset,
            maxItems: maxItems
        )
        return (
            projection.words.map {
                Word(
                    start: Double($0.startMilliseconds) / 1_000,
                    end: Double($0.endMilliseconds) / 1_000,
                    text: $0.text
                )
            },
            projection.hasMore
        )
    }

    func load(episodeID: UUID) -> Transcript? {
        try? loadThrowing(episodeID: episodeID)
    }

    func loadThrowing(episodeID: UUID) throws -> Transcript? {
        guard let summary = try summary(episodeID: episodeID) else { return nil }
        let source = try Self.nativeSource(summary.source)
        let speakers = try allPages { offset in
            try speakersPage(episodeID: episodeID, offset: offset)
        }
        var segments = try allPages { offset in
            try segmentsPage(episodeID: episodeID, offset: offset)
        }
        for index in segments.indices where segments[index].wordCount > 0 {
            let segment = segments[index]
            let words = try allPages { offset in
                try wordsPage(
                    episodeID: episodeID,
                    segmentID: segment.coreID,
                    offset: offset
                )
            }
            segments[index] = SharedTranscriptSegment(
                coreID: segment.coreID,
                ordinal: segment.ordinal,
                value: Segment(
                    id: segment.value.id,
                    start: segment.value.start,
                    end: segment.value.end,
                    speakerID: segment.value.speakerID,
                    text: segment.value.text,
                    words: words
                ),
                wordCount: segment.wordCount
            )
        }
        guard let id = summary.artifactId.uuid,
              summary.episodeId.uuid == episodeID else {
            throw SharedTranscriptReadError.invalidProjection
        }
        return Transcript(
            id: id,
            episodeID: episodeID,
            language: summary.language,
            source: source,
            segments: segments.sorted { $0.ordinal < $1.ordinal }.map(\.value),
            speakers: speakers,
            generatedAt: summary.generatedAt.date
        )
    }

    static func nativeSource(_ source: Pod0Core.TranscriptSource) throws -> TranscriptSource {
        switch source {
        case .publisher: .publisher
        case .scribe: .scribeV1
        case .whisper: .whisper
        case .onDevice: .onDevice
        case .assemblyAi: .assemblyAI
        case .other, .unsupported: throw SharedTranscriptReadError.unsupportedSource
        }
    }

    private func page(
        episodeID: UUID,
        scope: TranscriptProjectionScope,
        offset: UInt32,
        maxItems: UInt16
    ) throws -> TranscriptProjection {
        let limit = min(max(1, maxItems), Self.maximumPageSize)
        let envelope = facade.snapshot(request: ProjectionRequest(
            scope: .transcript(episodeId: EpisodeId(uuid: episodeID), scope: scope),
            offset: offset,
            maxItems: limit
        ))
        guard case .transcript(let projection) = envelope.projection else {
            throw SharedTranscriptReadError.invalidProjection
        }
        guard projection.failure == nil else { throw SharedTranscriptReadError.unavailable }
        return projection
    }

    private func segment(_ value: TranscriptSegmentProjection) throws -> SharedTranscriptSegment {
        guard let id = value.segmentId.uuid else {
            throw SharedTranscriptReadError.invalidProjection
        }
        return SharedTranscriptSegment(
            coreID: value.segmentId,
            ordinal: value.ordinal,
            value: Segment(
                id: id,
                start: Double(value.startMilliseconds) / 1_000,
                end: Double(value.endMilliseconds) / 1_000,
                speakerID: value.speakerId?.uuid,
                text: value.text,
                words: value.wordCount == 0 ? nil : []
            ),
            wordCount: value.wordCount
        )
    }

    private func allPages<Item>(
        _ load: (UInt32) throws -> (items: [Item], hasMore: Bool)
    ) throws -> [Item] {
        var offset: UInt32 = 0
        var result: [Item] = []
        while true {
            let page = try load(offset)
            result.append(contentsOf: page.items)
            guard page.hasMore,
                  offset <= UInt32.max - UInt32(Self.maximumPageSize) else { break }
            offset += UInt32(Self.maximumPageSize)
        }
        return result
    }
}
