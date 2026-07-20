import Foundation
import os.log

extension AgentTTSComposer {
    /// Converts the turn sequence into native raw observations. Rust qualifies
    /// and persists both transcript and chapter artifacts after composition.
    func buildChaptersAndTranscript(
        turns: [TTSTurn],
        trackDurations: [Double],
        episodeID: UUID
    ) async -> ([Episode.Chapter], Transcript) {
        var chapters: [Episode.Chapter] = []
        var transcriptSegments: [Segment] = []
        var cursor: TimeInterval = 0
        var speechStart: TimeInterval?
        var speechTexts: [String] = []

        func flushSpeechChapter() {
            guard !speechTexts.isEmpty, let start = speechStart else { return }
            let combinedText = speechTexts.joined(separator: " ")
            let preview = String(combinedText.prefix(60))
            let chapterTitle = combinedText.count <= 60 ? combinedText : preview + "…"
            chapters.append(Episode.Chapter(
                startTime: start,
                title: chapterTitle,
                isAIGenerated: true
            ))
            speechStart = nil
            speechTexts = []
        }

        for (index, turn) in turns.enumerated() {
            let duration = index < trackDurations.count ? trackDurations[index] : 0
            switch turn.kind {
            case .speech(let text, _):
                if speechStart == nil { speechStart = cursor }
                speechTexts.append(text)
                transcriptSegments.append(Segment(
                    start: cursor,
                    end: cursor + duration,
                    text: text
                ))
            case .snippet(let sourceID, let snippetStart, _, let label):
                flushSpeechChapter()
                let artworkURL = await MainActor.run { [weak self] () -> URL? in
                    guard let self, let store = self.store,
                          let uuid = UUID(uuidString: sourceID),
                          let episode = store.episode(id: uuid) else { return nil }
                    return episode.imageURL ?? store.podcast(id: episode.podcastID)?.imageURL
                }
                let chapterTitle: String
                if let label, !label.isEmpty {
                    chapterTitle = label
                } else if let resolved = await resolveEpisodeTitle(episodeID: sourceID) {
                    chapterTitle = resolved
                } else {
                    chapterTitle = String(
                        format: "Quote at %d:%02d",
                        Int(snippetStart) / 60,
                        Int(snippetStart) % 60
                    )
                }
                chapters.append(Episode.Chapter(
                    startTime: cursor,
                    title: chapterTitle,
                    imageURL: artworkURL,
                    isAIGenerated: true,
                    sourceEpisodeID: sourceID
                ))
                if let label, !label.isEmpty {
                    transcriptSegments.append(Segment(
                        start: cursor,
                        end: cursor + duration,
                        text: label
                    ))
                }
            }
            cursor += duration
        }
        flushSpeechChapter()

        return (chapters, Transcript(
            episodeID: episodeID,
            language: "en",
            source: .onDevice,
            segments: transcriptSegments
        ))
    }

    private func resolveEpisodeTitle(episodeID: String) async -> String? {
        await MainActor.run {
            guard let uuid = UUID(uuidString: episodeID),
                  let episode = store?.episode(id: uuid) else {
                Self.logger.error(
                    "AgentTTSComposer: episode not found for chapter title lookup — episodeID=\(episodeID, privacy: .public)"
                )
                return nil
            }
            return episode.title
        }
    }
}
