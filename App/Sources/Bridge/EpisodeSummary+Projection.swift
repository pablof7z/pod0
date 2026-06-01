import Foundation

// MARK: - EpisodeSummary → Episode mapping
//
// Wire-to-domain mapping for the kernel projection. Extracted from
// `AppStateStore+KernelProjection.swift` to keep that file under the 500-line
// hard limit (AGENTS.md), as flagged by the `kernelprojection-split` backlog
// item. `internal` (not `private`) so `applyKernelState` in the projection
// file can call `toEpisode`/`toChapter` across the file boundary.
//
// NOTE (future merge): `fix/file-size-projection` edits `toEpisode`'s
// download-size block in the original file location. When that branch lands,
// reconcile its hunk against this relocated copy.

extension EpisodeSummary {
    func toEpisode(podcastIdString: String) -> Episode? {
        guard let episodeUUID = UUID(uuidString: id),
              let podcastUUID = UUID(uuidString: podcastIdString)
        else { return nil }

        let pubDate: Date = publishedAt.map { Date(timeIntervalSince1970: Double($0)) } ?? Date.distantPast

        // For downloaded episodes, use the local file URL. For streaming
        // episodes, use the RSS enclosure URL projected from Rust so the
        // host player can start without a Rust round-trip.
        let enclosureURL: URL = downloadPath.flatMap { URL(fileURLWithPath: $0) }
            ?? enclosureUrl.flatMap { URL(string: $0) }
            ?? URL(string: "https://placeholder.invalid/\(id)")!

        let downloadState: DownloadState
        if let path = downloadPath {
            let fileURL = URL(fileURLWithPath: path)
            // Size is cached by the Rust kernel at download-completion time
            // (`EpisodeSummary.file_size_bytes`), so we avoid a synchronous
            // `URL.resourceValues(.fileSizeKey)` stat on the main actor for
            // every downloaded episode on every projection tick.
            let byteCount: Int64 = fileSizeBytes
            downloadState = .downloaded(localFileURL: fileURL, byteCount: byteCount)
        } else {
            downloadState = .notDownloaded
        }

        let projectedChapters: [Episode.Chapter]? = chapters.flatMap {
            $0.isEmpty ? nil : $0.map(\.toChapter)
        }
        let projectedAdSegments: [Episode.AdSegment]? = adSegments.isEmpty ? nil : adSegments.compactMap { seg in
            guard let uuid = UUID(uuidString: seg.id) else { return nil }
            let kind = Episode.AdKind(rawValue: seg.kind) ?? .midroll
            return Episode.AdSegment(id: uuid, start: seg.startSecs, end: seg.endSecs, kind: kind)
        }
        // Derive transcriptState entirely from the Rust projection (M4 / D7).
        //   1. A non-empty stored `transcript` ⇒ `.ready`. It came from either
        //      iOS STT (kernelTranscriptReport) or a publisher fetch; we can't
        //      distinguish the source from Rust alone, so use `.publisher` as
        //      the conservative default (the precise source lives on the iOS
        //      TranscriptStore for the badge).
        //   2. Otherwise honour the transient status iOS reported via
        //      `set_episode_transcript_status` (queued / fetching publisher /
        //      transcribing / failed). The progress arg is always 0 — the real
        //      pipeline never streams a percentage (it sets `.transcribing(0)`
        //      once before the provider call), so no progress round-trips.
        //   3. No transcript and no override ⇒ `.none`.
        let derivedTranscriptState: TranscriptState? = {
            if let text = transcript, !text.isEmpty {
                return .ready(source: .publisher)
            }
            switch transcriptStatus {
            case "queued": return .queued
            case "fetching_publisher": return .fetchingPublisher
            case "transcribing": return .transcribing(progress: 0)
            case "failed":
                return .failed(message: transcriptStatusMessage ?? "Transcription didn't finish.")
            default: return nil
            }
        }()

        return Episode(
            id: episodeUUID,
            podcastID: podcastUUID,
            guid: id,
            title: title,
            description: description ?? "",
            pubDate: pubDate,
            duration: durationSecs,
            enclosureURL: enclosureURL,
            imageURL: artworkUrl.flatMap { URL(string: $0) },
            chapters: projectedChapters,
            publisherTranscriptURL: transcriptUrl.flatMap { URL(string: $0) },
            playbackPosition: playbackPositionSecs ?? 0,
            played: played,
            isStarred: starred,
            downloadState: downloadState,
            transcriptState: derivedTranscriptState ?? .none,
            adSegments: projectedAdSegments,
            // M4 / D7: all three derive from the Rust projection now — no
            // preserved-state merge. `triageDecision` parses the rawValue
            // ("inbox" / "archived"); an absent / unrecognised value ⇒ nil
            // (untriaged).
            triageDecision: triageDecision.flatMap { TriageDecision(rawValue: $0) },
            triageRationale: triageRationale,
            triageIsHero: triageIsHero,
            metadataIndexed: metadataIndexed,
            // #45: AI-generated category labels. Projection-only — the
            // kernel owns them, so they ride the snapshot straight onto the
            // domain model with no preserved-state merge.
            aiCategories: aiCategories,
            // AI episode summary. Projection-only — produced by the kernel
            // `summarize_episode` pass and carried straight onto the domain
            // model so `store.episode(id:).summary` reflects it.
            summary: summary
        )
    }
}

// MARK: - ChapterSummary → Episode.Chapter

extension ChapterSummary {
    var toChapter: Episode.Chapter {
        Episode.Chapter(
            startTime: startSecs,
            endTime: endSecs,
            title: title,
            imageURL: imageUrl.flatMap { URL(string: $0) },
            linkURL: url.flatMap { URL(string: $0) },
            isAIGenerated: isAiGenerated,
            sourceEpisodeID: sourceEpisodeId
        )
    }
}
