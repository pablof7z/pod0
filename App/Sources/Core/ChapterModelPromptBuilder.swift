import Foundation
import Pod0Core

/// Temporary Swift prompt presentation for #100. The model completion stays
/// opaque in native code and is qualified by Rust; this builder is deleted
/// when the shared workflow kernel emits the typed model host request.
struct ChapterModelPrompt: Sendable, Equatable {
    let system: String
    let user: String
}

enum ChapterModelPromptBuilder {
    static let maximumTranscriptCharacters = 28_000

    static func make(
        episode: Episode,
        transcript: Transcript,
        publisherChapters: [ChapterInput]?
    ) -> ChapterModelPrompt {
        if let publisherChapters, !publisherChapters.isEmpty {
            return ChapterModelPrompt(
                system: enrichmentSystemPrompt,
                user: enrichmentUserPrompt(
                    episode: episode,
                    transcript: transcript,
                    chapters: publisherChapters
                )
            )
        }
        return ChapterModelPrompt(
            system: generationSystemPrompt,
            user: generationUserPrompt(episode: episode, transcript: transcript)
        )
    }

    private static func generationUserPrompt(
        episode: Episode,
        transcript: Transcript
    ) -> String {
        """
        \(durationLine(episode))Title: \(episode.title)
        Transcript (timestamped):
        \(transcriptBody(transcript))
        """
    }

    private static func enrichmentUserPrompt(
        episode: Episode,
        transcript: Transcript,
        chapters: [ChapterInput]
    ) -> String {
        let chapterLines = chapters.enumerated().map { index, chapter in
            "[\(index)] \(chapter.startMilliseconds / 1_000)s — \(chapter.title)"
        }.joined(separator: "\n")
        return """
        \(durationLine(episode))Title: \(episode.title)
        Existing chapters (use these exact indices in your "summaries" output):
        \(chapterLines)
        Transcript (timestamped):
        \(transcriptBody(transcript))
        """
    }

    private static func durationLine(_ episode: Episode) -> String {
        episode.duration.map { "Episode duration: \(Int($0)) seconds.\n" } ?? ""
    }

    private static func transcriptBody(_ transcript: Transcript) -> String {
        var body = transcript.segments.map { segment in
            let timestamp = Int(segment.start.rounded())
            let text = segment.text.trimmingCharacters(in: .whitespacesAndNewlines)
            return "[\(timestamp)s] \(text)"
        }.joined(separator: "\n")
        if body.count > maximumTranscriptCharacters {
            body = String(body.prefix(maximumTranscriptCharacters))
        }
        return body
    }

    private static let generationSystemPrompt = """
    You analyse podcast episode transcripts and return chapter boundaries, \
    chapter summaries, and advertisement spans in a single JSON response. \
    Always respond with ONLY this JSON object (no prose, no markdown fences):
    {
      "chapters": [
        { "start": <seconds>, "title": "<short title>", "summary": "<1-2 sentence summary>" }
      ],
      "ads": [
        { "start": <seconds>, "end": <seconds>, "kind": "preroll"|"midroll"|"postroll" }
      ]
    }
    Chapter rules:
      - Produce between 4 and 12 chapters total.
      - "start" is seconds from the beginning of the episode, integer or float.
      - The first chapter must start at 0.
      - Chapters must be strictly monotonic by "start".
      - Titles are short (max 6 words), descriptive, no quotes, no episode numbers.
      - "summary" is 1-2 sentences describing what the chapter covers.
      - Skip ad reads; do not create a chapter for them.
      - Prefer topic shifts over speaker changes.
    Ad rules:
      - Only mark spans that are clearly advertisements.
      - Do not mark guest plugs, book recommendations, or off-topic asides.
      - "end" must be greater than "start"; ranges must not overlap.
      - Use "preroll" before topical content, "postroll" after, otherwise "midroll".
      - Return an empty "ads" array when the episode has no ads.
    """

    private static let enrichmentSystemPrompt = """
    You analyse podcast episode transcripts. The episode already has publisher \
    chapter boundaries. Return ONLY this JSON object (no prose or markdown):
    {
      "summaries": [
        { "index": <int>, "summary": "<1-2 sentence summary>" }
      ],
      "ads": [
        { "start": <seconds>, "end": <seconds>, "kind": "preroll"|"midroll"|"postroll" }
      ]
    }
    Summary rules:
      - Return one entry per supplied chapter using its exact index.
      - Do not change titles or invent chapters.
    Ad rules:
      - Only mark spans that are clearly advertisements.
      - Do not mark guest plugs, book recommendations, or off-topic asides.
      - "end" must be greater than "start"; ranges must not overlap.
      - Use "preroll" before topical content, "postroll" after, otherwise "midroll".
      - Return an empty "ads" array when the episode has no ads.
    """
}
