use crate::{
    AdSpanEvaluation, ChapterAdKind, ChapterArtifactError, ChapterArtifactInput,
    ChapterArtifactSource, EpisodeId, MAX_AD_SPANS, MAX_CHAPTER_ARTIFACT_BYTES,
    MAX_CHAPTER_MODEL_BYTES, MAX_CHAPTER_SUMMARY_BYTES, MAX_CHAPTER_TITLE_BYTES,
    MAX_CHAPTER_URL_BYTES, MAX_CHAPTERS, MAX_PROVENANCE_PROVIDER_BYTES, MAX_SOURCE_REVISION_BYTES,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CanonicalChapter {
    pub ordinal: u32,
    pub start_milliseconds: u64,
    pub end_milliseconds: Option<u64>,
    pub title: String,
    pub summary: Option<String>,
    pub image_url: Option<String>,
    pub link_url: Option<String>,
    pub include_in_table_of_contents: bool,
    pub source_episode_id: Option<EpisodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CanonicalAdSpan {
    pub ordinal: u32,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub kind: ChapterAdKind,
}

pub(crate) fn validate_metadata(input: &ChapterArtifactInput) -> Result<(), ChapterArtifactError> {
    if invalid_exact_text(&input.source_revision, MAX_SOURCE_REVISION_BYTES)
        || input.generated_at.value < 0
        || input.duration_milliseconds == Some(0)
    {
        return Err(ChapterArtifactError::InvalidMetadata);
    }
    validate_optional_exact_text(
        input.provenance.provider.as_deref(),
        MAX_PROVENANCE_PROVIDER_BYTES,
    )?;
    validate_optional_exact_text(input.provenance.model.as_deref(), MAX_CHAPTER_MODEL_BYTES)?;
    validate_provenance(input)
}

pub(crate) fn canonical_chapters(
    input: &ChapterArtifactInput,
) -> Result<Vec<CanonicalChapter>, ChapterArtifactError> {
    if input.chapters.is_empty() {
        return Err(ChapterArtifactError::InvalidChapter);
    }
    if input.chapters.len() > MAX_CHAPTERS {
        return Err(ChapterArtifactError::TooManyChapters);
    }
    let mut total_bytes = 0_usize;
    let mut chapters = Vec::with_capacity(input.chapters.len());
    for (index, source) in input.chapters.iter().enumerate() {
        let ordinal = u32::try_from(index).map_err(|_| ChapterArtifactError::TooManyChapters)?;
        if source.title.trim().is_empty() {
            return Err(ChapterArtifactError::InvalidChapter);
        }
        if source.title.len() > MAX_CHAPTER_TITLE_BYTES
            || source
                .summary
                .as_ref()
                .is_some_and(|value| value.len() > MAX_CHAPTER_SUMMARY_BYTES)
            || invalid_optional_url(source.image_url.as_deref())
            || invalid_optional_url(source.link_url.as_deref())
        {
            return Err(ChapterArtifactError::TextLimit);
        }
        let title = normalized_text(&source.title);
        let summary = source.summary.as_ref().map(|value| normalized_text(value));
        if summary.as_ref().is_some_and(String::is_empty) {
            return Err(ChapterArtifactError::InvalidChapter);
        }
        if !valid_range(
            source.start_milliseconds,
            source.end_milliseconds,
            input.duration_milliseconds,
        ) {
            return Err(ChapterArtifactError::InvalidChapter);
        }
        total_bytes = total_bytes
            .saturating_add(title.len())
            .saturating_add(summary.as_ref().map_or(0, String::len))
            .saturating_add(source.image_url.as_ref().map_or(0, String::len))
            .saturating_add(source.link_url.as_ref().map_or(0, String::len));
        if total_bytes > MAX_CHAPTER_ARTIFACT_BYTES {
            return Err(ChapterArtifactError::ArtifactTooLarge);
        }
        chapters.push(CanonicalChapter {
            ordinal,
            start_milliseconds: source.start_milliseconds,
            end_milliseconds: source.end_milliseconds,
            title,
            summary,
            image_url: source.image_url.clone(),
            link_url: source.link_url.clone(),
            include_in_table_of_contents: source.include_in_table_of_contents,
            source_episode_id: source.source_episode_id,
        });
    }
    for pair in chapters.windows(2) {
        if pair[0].start_milliseconds >= pair[1].start_milliseconds {
            return Err(ChapterArtifactError::ChaptersOutOfOrder);
        }
        if pair[0]
            .end_milliseconds
            .is_some_and(|end| end > pair[1].start_milliseconds)
        {
            return Err(ChapterArtifactError::ChaptersOverlap);
        }
    }
    Ok(chapters)
}

pub(crate) fn canonical_ad_spans(
    input: &ChapterArtifactInput,
) -> Result<Vec<CanonicalAdSpan>, ChapterArtifactError> {
    match input.ad_span_evaluation {
        AdSpanEvaluation::NotEvaluated if !input.ad_spans.is_empty() => {
            return Err(ChapterArtifactError::InvalidAdSpan);
        }
        AdSpanEvaluation::Unsupported { wire_code } => {
            return Err(ChapterArtifactError::UnsupportedAdEvaluation { wire_code });
        }
        AdSpanEvaluation::NotEvaluated | AdSpanEvaluation::Evaluated => {}
    }
    if input.ad_spans.len() > MAX_AD_SPANS {
        return Err(ChapterArtifactError::TooManyAdSpans);
    }
    let mut spans = Vec::with_capacity(input.ad_spans.len());
    for (index, source) in input.ad_spans.iter().enumerate() {
        let ordinal = u32::try_from(index).map_err(|_| ChapterArtifactError::TooManyAdSpans)?;
        if let ChapterAdKind::Unsupported { wire_code } = source.kind {
            return Err(ChapterArtifactError::UnsupportedAdKind { wire_code });
        }
        if !valid_range(
            source.start_milliseconds,
            Some(source.end_milliseconds),
            input.duration_milliseconds,
        ) {
            return Err(ChapterArtifactError::InvalidAdSpan);
        }
        spans.push(CanonicalAdSpan {
            ordinal,
            start_milliseconds: source.start_milliseconds,
            end_milliseconds: source.end_milliseconds,
            kind: source.kind,
        });
    }
    for pair in spans.windows(2) {
        if pair[0].start_milliseconds >= pair[1].start_milliseconds {
            return Err(ChapterArtifactError::AdSpansOutOfOrder);
        }
        if pair[0].end_milliseconds > pair[1].start_milliseconds {
            return Err(ChapterArtifactError::AdSpansOverlap);
        }
    }
    Ok(spans)
}

fn validate_provenance(input: &ChapterArtifactInput) -> Result<(), ChapterArtifactError> {
    let provenance = &input.provenance;
    let transcript_pair = (
        provenance.transcript_version_id.is_some(),
        provenance.transcript_content_digest.is_some(),
    );
    if !matches!(transcript_pair, (false, false) | (true, true)) {
        return Err(ChapterArtifactError::InvalidProvenance);
    }
    match provenance.source {
        ChapterArtifactSource::Publisher
            if provenance.policy_version == 0
                && transcript_pair == (false, false)
                && provenance.model.is_none() =>
        {
            Ok(())
        }
        ChapterArtifactSource::Generated | ChapterArtifactSource::PublisherEnriched
            if provenance.policy_version > 0
                && transcript_pair == (true, true)
                && provenance.provider.is_some()
                && provenance.model.is_some() =>
        {
            Ok(())
        }
        ChapterArtifactSource::AgentComposed if provenance.policy_version > 0 => Ok(()),
        ChapterArtifactSource::Unsupported { wire_code } => {
            Err(ChapterArtifactError::UnsupportedSource { wire_code })
        }
        _ => Err(ChapterArtifactError::InvalidProvenance),
    }
}

fn validate_optional_exact_text(
    value: Option<&str>,
    maximum: usize,
) -> Result<(), ChapterArtifactError> {
    if value.is_some_and(|value| invalid_exact_text(value, maximum)) {
        Err(ChapterArtifactError::InvalidProvenance)
    } else {
        Ok(())
    }
}

fn valid_range(start: u64, end: Option<u64>, duration: Option<u64>) -> bool {
    !(end.is_some_and(|end| end <= start)
        || duration.is_some_and(|duration| start >= duration)
        || end
            .zip(duration)
            .is_some_and(|(end, duration)| end > duration))
}

fn invalid_exact_text(value: &str, maximum: usize) -> bool {
    value.is_empty() || value.trim() != value || value.len() > maximum
}

fn invalid_optional_url(value: Option<&str>) -> bool {
    value.is_some_and(|value| invalid_exact_text(value, MAX_CHAPTER_URL_BYTES))
}

fn normalized_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
