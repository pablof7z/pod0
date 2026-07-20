use crate::{
    ChapterModelEpisodeInput, ChapterModelTranscriptInput, MAX_CHAPTER_MODEL_TRANSCRIPT_CHARACTERS,
};
use pod0_domain::ChapterInput;
use unicode_segmentation::UnicodeSegmentation as _;

pub(crate) const GENERATION_SYSTEM_PROMPT: &str = concat!(
    "You analyse podcast episode transcripts and return chapter boundaries, ",
    "chapter summaries, and advertisement spans in a single JSON response. ",
    "Always respond with ONLY this JSON object (no prose, no markdown fences):\n",
    "{\n",
    "  \"chapters\": [\n",
    "    { \"start\": <seconds>, \"title\": \"<short title>\", ",
    "\"summary\": \"<1-2 sentence summary>\" }\n",
    "  ],\n",
    "  \"ads\": [\n",
    "    { \"start\": <seconds>, \"end\": <seconds>, ",
    "\"kind\": \"preroll\"|\"midroll\"|\"postroll\" }\n",
    "  ]\n",
    "}\n",
    "Chapter rules:\n",
    "  - Produce between 4 and 12 chapters total.\n",
    "  - \"start\" is seconds from the beginning of the episode, integer or float.\n",
    "  - The first chapter must start at 0.\n",
    "  - Chapters must be strictly monotonic by \"start\".\n",
    "  - Titles are short (max 6 words), descriptive, no quotes, no episode numbers.\n",
    "  - \"summary\" is 1-2 sentences describing what the chapter covers.\n",
    "  - Skip ad reads; do not create a chapter for them.\n",
    "  - Prefer topic shifts over speaker changes.\n",
    "Ad rules:\n",
    "  - Only mark spans that are clearly advertisements.\n",
    "  - Do not mark guest plugs, book recommendations, or off-topic asides.\n",
    "  - \"end\" must be greater than \"start\"; ranges must not overlap.\n",
    "  - Use \"preroll\" before topical content, \"postroll\" after, otherwise \"midroll\".\n",
    "  - Return an empty \"ads\" array when the episode has no ads."
);

pub(crate) const ENRICHMENT_SYSTEM_PROMPT: &str = concat!(
    "You analyse podcast episode transcripts. The episode already has publisher ",
    "chapter boundaries. Return ONLY this JSON object (no prose or markdown):\n",
    "{\n",
    "  \"summaries\": [\n",
    "    { \"index\": <int>, \"summary\": \"<1-2 sentence summary>\" }\n",
    "  ],\n",
    "  \"ads\": [\n",
    "    { \"start\": <seconds>, \"end\": <seconds>, ",
    "\"kind\": \"preroll\"|\"midroll\"|\"postroll\" }\n",
    "  ]\n",
    "}\n",
    "Summary rules:\n",
    "  - Return one entry per supplied chapter using its exact index.\n",
    "  - Do not change titles or invent chapters.\n",
    "Ad rules:\n",
    "  - Only mark spans that are clearly advertisements.\n",
    "  - Do not mark guest plugs, book recommendations, or off-topic asides.\n",
    "  - \"end\" must be greater than \"start\"; ranges must not overlap.\n",
    "  - Use \"preroll\" before topical content, \"postroll\" after, otherwise \"midroll\".\n",
    "  - Return an empty \"ads\" array when the episode has no ads."
);

pub(crate) fn generation_user_prompt(
    episode: &ChapterModelEpisodeInput,
    transcript: &ChapterModelTranscriptInput,
) -> String {
    format!(
        "{}Title: {}\nTranscript (timestamped):\n{}",
        duration_line(episode),
        episode.title,
        transcript_body(transcript)
    )
}

pub(crate) fn enrichment_user_prompt(
    episode: &ChapterModelEpisodeInput,
    transcript: &ChapterModelTranscriptInput,
    chapters: &[ChapterInput],
) -> String {
    let chapter_lines = chapters
        .iter()
        .enumerate()
        .map(|(index, chapter)| {
            format!(
                "[{index}] {}s — {}",
                chapter.start_milliseconds / 1_000,
                chapter.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{}Title: {}\nExisting chapters (use these exact indices in your \
         \"summaries\" output):\n{}\nTranscript (timestamped):\n{}",
        duration_line(episode),
        episode.title,
        chapter_lines,
        transcript_body(transcript)
    )
}

fn duration_line(episode: &ChapterModelEpisodeInput) -> String {
    episode
        .duration_seconds
        .map_or_else(String::new, |duration| {
            format!("Episode duration: {} seconds.\n", duration.trunc() as u64)
        })
}

fn transcript_body(transcript: &ChapterModelTranscriptInput) -> String {
    let body = transcript
        .segments
        .iter()
        .map(|segment| {
            format!(
                "[{}s] {}",
                segment.start_seconds.round() as i64,
                segment.text.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if body.graphemes(true).count() > MAX_CHAPTER_MODEL_TRANSCRIPT_CHARACTERS {
        body.graphemes(true)
            .take(MAX_CHAPTER_MODEL_TRANSCRIPT_CHARACTERS)
            .collect()
    } else {
        body
    }
}
