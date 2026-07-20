import Foundation
import Pod0Core

enum SharedChapterReadError: Error, Equatable {
    case unavailable
    case invalidProjection
    case unsupportedValue
}

struct SharedChapterSnapshot: Sendable, Equatable {
    let summary: ChapterSummaryProjection
    let chapters: [Episode.Chapter]
    let adSegments: [Episode.AdSegment]?
}

struct SharedChapterArtifactInput: Sendable, Equatable {
    let artifact: ChapterArtifactInput
    let selectionRevision: StateRevision
}

/// Native presentation adapter over the Rust-authoritative chapter store.
/// Every FFI read is page-bounded; the reconstructed arrays are transient.
final class SharedChapterReader: @unchecked Sendable {
    static let maximumPageSize: UInt16 = 200
    static let maximumMaterializedItems = 4_000

    private let facade: Pod0Facade

    init(facade: Pod0Facade) {
        self.facade = facade
    }

    func summary(episodeID: UUID) throws -> ChapterSummaryProjection? {
        try page(episodeID: episodeID, scope: .summary, offset: 0, maxItems: 1).summary
    }

    func load(episodeID: UUID) throws -> SharedChapterSnapshot? {
        guard let summary = try summary(episodeID: episodeID) else { return nil }
        guard summary.episodeId.uuid == episodeID else {
            throw SharedChapterReadError.invalidProjection
        }
        let items = try allPages { offset in
            let projection = try page(
                episodeID: episodeID,
                scope: .chapters,
                offset: offset,
                maxItems: Self.maximumPageSize
            )
            try requireSelection(summary.selectionRevision, in: projection)
            return (projection.chapters, projection.hasMore)
        }
        let generated = try isAIGenerated(summary.provenance.source)
        let chapters = try items.map { item -> Episode.Chapter in
            guard let id = item.chapterId.uuid else {
                throw SharedChapterReadError.invalidProjection
            }
            return Episode.Chapter(
                id: id,
                startTime: Double(item.startMilliseconds) / 1_000,
                endTime: item.explicitEndMilliseconds.map { Double($0) / 1_000 },
                title: item.title,
                imageURL: item.imageUrl.flatMap(URL.init(string:)),
                linkURL: item.linkUrl.flatMap(URL.init(string:)),
                includeInTableOfContents: item.includeInTableOfContents,
                isAIGenerated: generated,
                summary: item.summary,
                sourceEpisodeID: item.sourceEpisodeId?.uuid?.uuidString
            )
        }
        let adSegments: [Episode.AdSegment]?
        switch summary.adSpanEvaluation {
        case .notEvaluated:
            adSegments = nil
        case .evaluated:
            let spans = try allPages { offset in
                let projection = try page(
                    episodeID: episodeID,
                    scope: .adSpans,
                    offset: offset,
                    maxItems: Self.maximumPageSize
                )
                try requireSelection(summary.selectionRevision, in: projection)
                return (projection.adSpans, projection.hasMore)
            }
            adSegments = try spans.map { span in
                guard let id = span.adSpanId.uuid else {
                    throw SharedChapterReadError.invalidProjection
                }
                return Episode.AdSegment(
                    id: id,
                    start: Double(span.startMilliseconds) / 1_000,
                    end: Double(span.endMilliseconds) / 1_000,
                    kind: try adKind(span.kind)
                )
            }
        case .unsupported:
            throw SharedChapterReadError.unsupportedValue
        }
        return SharedChapterSnapshot(
            summary: summary,
            chapters: chapters,
            adSegments: adSegments
        )
    }

    /// Reconstructs the selected domain input for a typed Rust enrichment
    /// observation. This value is transient and page-bounded at the FFI seam;
    /// it is never persisted by Swift.
    func selectedArtifactInput(episodeID: UUID) throws -> SharedChapterArtifactInput? {
        guard let summary = try summary(episodeID: episodeID) else { return nil }
        guard summary.episodeId.uuid == episodeID else {
            throw SharedChapterReadError.invalidProjection
        }
        let chapters = try allPages { offset in
            let projection = try page(
                episodeID: episodeID,
                scope: .chapters,
                offset: offset,
                maxItems: Self.maximumPageSize
            )
            try requireSelection(summary.selectionRevision, in: projection)
            return (projection.chapters, projection.hasMore)
        }.map { item in
            ChapterInput(
                startMilliseconds: item.startMilliseconds,
                endMilliseconds: item.explicitEndMilliseconds,
                title: item.title,
                summary: item.summary,
                imageUrl: item.imageUrl,
                linkUrl: item.linkUrl,
                includeInTableOfContents: item.includeInTableOfContents,
                sourceEpisodeId: item.sourceEpisodeId
            )
        }
        let adSpans = try allPages { offset in
            let projection = try page(
                episodeID: episodeID,
                scope: .adSpans,
                offset: offset,
                maxItems: Self.maximumPageSize
            )
            try requireSelection(summary.selectionRevision, in: projection)
            return (projection.adSpans, projection.hasMore)
        }.map {
            AdSpanInput(
                startMilliseconds: $0.startMilliseconds,
                endMilliseconds: $0.endMilliseconds,
                kind: $0.kind
            )
        }
        return SharedChapterArtifactInput(
            artifact: ChapterArtifactInput(
                episodeId: summary.episodeId,
                podcastId: summary.podcastId,
                sourceRevision: summary.sourceRevision,
                provenance: summary.provenance,
                generatedAt: summary.generatedAt,
                durationMilliseconds: summary.durationMilliseconds,
                chapters: chapters,
                adSpanEvaluation: summary.adSpanEvaluation,
                adSpans: adSpans
            ),
            selectionRevision: summary.selectionRevision
        )
    }

    private func requireSelection(
        _ revision: StateRevision,
        in projection: ChapterArtifactProjection
    ) throws {
        guard projection.summary?.selectionRevision == revision else {
            throw SharedChapterReadError.invalidProjection
        }
    }

    private func page(
        episodeID: UUID,
        scope: ChapterProjectionScope,
        offset: UInt32,
        maxItems: UInt16
    ) throws -> ChapterArtifactProjection {
        let envelope = facade.snapshot(request: ProjectionRequest(
            scope: .chapter(episodeId: EpisodeId(uuid: episodeID), scope: scope),
            offset: offset,
            maxItems: min(max(1, maxItems), Self.maximumPageSize)
        ))
        guard case .chapter(let projection) = envelope.projection else {
            throw SharedChapterReadError.invalidProjection
        }
        guard projection.failure == nil else { throw SharedChapterReadError.unavailable }
        return projection
    }

    private func allPages<Item>(
        _ load: (UInt32) throws -> (items: [Item], hasMore: Bool)
    ) throws -> [Item] {
        var offset: UInt32 = 0
        var result: [Item] = []
        while true {
            let page = try load(offset)
            guard page.items.count <= Self.maximumMaterializedItems - result.count,
                  !page.hasMore || !page.items.isEmpty else {
                throw SharedChapterReadError.invalidProjection
            }
            result.append(contentsOf: page.items)
            guard page.hasMore else { break }
            guard let consumed = UInt32(exactly: page.items.count),
                  offset <= UInt32.max - consumed else {
                throw SharedChapterReadError.invalidProjection
            }
            offset += consumed
        }
        return result
    }

    private func isAIGenerated(_ source: ChapterArtifactSource) throws -> Bool {
        switch source {
        case .publisher, .publisherEnriched: false
        case .generated, .agentComposed: true
        case .unsupported: throw SharedChapterReadError.unsupportedValue
        }
    }

    private func adKind(_ kind: ChapterAdKind) throws -> Episode.AdKind {
        switch kind {
        case .preroll: .preroll
        case .midroll: .midroll
        case .postroll: .postroll
        case .unsupported: throw SharedChapterReadError.unsupportedValue
        }
    }
}
