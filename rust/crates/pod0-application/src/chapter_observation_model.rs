use pod0_domain::{
    AdSpanEvaluation, AdSpanInput, ChapterAdKind, ChapterArtifact, ChapterArtifactInput,
    ChapterArtifactProvenance, ChapterArtifactSource, ChapterInput, MAX_AD_SPANS, MAX_CHAPTERS,
    MAX_SOURCE_REVISION_BYTES,
};
use serde::Deserialize;

use crate::chapter_observation_values::{
    ObservationHash, canonicalize, clamped_milliseconds, exact_model, exact_provider,
    payload_digest,
};
use crate::{
    CHAPTER_MODEL_FORMAT_VERSION, CHAPTER_OBSERVATION_POLICY_VERSION, ChapterModelObservationMode,
    ChapterObservationRejection, ModelChapterObservation, Qualification, QualifiedObservation,
};

const MIN_GENERATED_CHAPTERS: usize = 4;
const MAX_GENERATED_CHAPTERS: usize = 12;

#[derive(Deserialize)]
struct GeneratedPayload {
    chapters: Vec<GeneratedChapter>,
    ads: Option<Vec<ModelAd>>,
}

#[derive(Deserialize)]
struct GeneratedChapter {
    start: f64,
    title: String,
    summary: Option<String>,
}

#[derive(Deserialize)]
struct EnrichmentPayload {
    summaries: Option<Vec<ModelSummary>>,
    ads: Option<Vec<ModelAd>>,
}

#[derive(Deserialize)]
struct ModelSummary {
    index: i64,
    summary: String,
}

#[derive(Deserialize)]
struct ModelAd {
    start: Option<f64>,
    end: Option<f64>,
    start_seconds: Option<f64>,
    end_seconds: Option<f64>,
    kind: Option<String>,
}

pub(crate) fn qualify(observation: ModelChapterObservation) -> Qualification {
    if observation.completion.len() > crate::MAX_MODEL_CHAPTER_COMPLETION_BYTES {
        return Err(ChapterObservationRejection::PayloadTooLarge);
    }
    if payload_digest(observation.completion.as_bytes()) != observation.completion_digest {
        return Err(ChapterObservationRejection::DigestMismatch);
    }
    if observation.format_version != CHAPTER_MODEL_FORMAT_VERSION {
        return Err(ChapterObservationRejection::UnsupportedFormat {
            format_version: observation.format_version,
        });
    }
    if observation.policy_version != CHAPTER_OBSERVATION_POLICY_VERSION {
        return Err(ChapterObservationRejection::UnsupportedPolicy {
            policy_version: observation.policy_version,
        });
    }
    if observation.requested_transcript_version_id != observation.selected_transcript_version_id
        || observation.requested_transcript_content_digest
            != observation.selected_transcript_content_digest
    {
        return Err(ChapterObservationRejection::StaleTranscriptEvidence);
    }
    let provider = exact_provider(&observation.provider)?;
    let model = exact_model(&observation.model)?;
    let source_version = observation.source_version.trim();
    if source_version.is_empty()
        || source_version != observation.source_version
        || source_version.len() > MAX_SOURCE_REVISION_BYTES
    {
        return Err(ChapterObservationRejection::InvalidProvenance);
    }

    let (source, duration, chapters, ads, mode_tag, base_integrity) = match observation.mode {
        ChapterModelObservationMode::Generate => {
            let payload: GeneratedPayload = serde_json::from_str(&observation.completion)
                .map_err(|_| ChapterObservationRejection::MalformedPayload)?;
            ensure_model_counts(payload.chapters.len(), payload.ads.as_ref().map(Vec::len))?;
            let chapters = generated_chapters(payload.chapters, observation.duration_milliseconds)?;
            let ads = model_ads(
                payload.ads.unwrap_or_default(),
                observation.duration_milliseconds,
            )?;
            (
                ChapterArtifactSource::Generated,
                observation.duration_milliseconds,
                chapters,
                ads,
                0_u8,
                None,
            )
        }
        ChapterModelObservationMode::Enrich { publisher_artifact } => {
            let base = ChapterArtifact::seal(publisher_artifact)
                .map_err(|_| ChapterObservationRejection::InvalidBaseArtifact)?;
            if base.episode_id != observation.episode_id
                || base.podcast_id != observation.podcast_id
                || !matches!(
                    base.provenance.source,
                    ChapterArtifactSource::Publisher | ChapterArtifactSource::PublisherEnriched
                )
            {
                return Err(ChapterObservationRejection::InvalidBaseArtifact);
            }
            let duration = merge_duration(
                observation.duration_milliseconds,
                base.duration_milliseconds,
            )?;
            let payload: EnrichmentPayload = serde_json::from_str(&observation.completion)
                .map_err(|_| ChapterObservationRejection::MalformedPayload)?;
            ensure_model_counts(
                payload.summaries.as_ref().map_or(0, Vec::len),
                payload.ads.as_ref().map(Vec::len),
            )?;
            let mut chapters = base.as_input().chapters;
            apply_summaries(&mut chapters, payload.summaries.unwrap_or_default());
            let ads = model_ads(payload.ads.unwrap_or_default(), duration)?;
            (
                ChapterArtifactSource::PublisherEnriched,
                duration,
                chapters,
                ads,
                1_u8,
                Some(base.integrity_digest),
            )
        }
    };

    let mut hash = ObservationHash::new(b"pod0.model-chapter-observation.v1");
    hash.bytes(&observation.episode_id.into_bytes());
    hash.bytes(&observation.podcast_id.into_bytes());
    hash.u32(observation.format_version);
    hash.u32(observation.policy_version);
    hash.text(&provider);
    hash.text(&model);
    hash.bytes(&observation.selected_transcript_version_id.into_bytes());
    hash.bytes(&observation.selected_transcript_content_digest.into_bytes());
    hash.text(source_version);
    hash.bytes(&observation.completion_digest.into_bytes());
    hash.optional_u64(duration);
    hash.u8(mode_tag);
    hash.optional_digest(base_integrity);
    let fingerprint = hash.finish();
    let artifact = canonicalize(ChapterArtifactInput {
        episode_id: observation.episode_id,
        podcast_id: observation.podcast_id,
        source_revision: source_version.to_owned(),
        provenance: ChapterArtifactProvenance {
            source,
            provider: Some(provider),
            model: Some(model),
            policy_version: observation.policy_version,
            source_payload_digest: observation.completion_digest,
            transcript_version_id: Some(observation.selected_transcript_version_id),
            transcript_content_digest: Some(observation.selected_transcript_content_digest),
            legacy_import: None,
        },
        generated_at: observation.generated_at,
        duration_milliseconds: duration,
        chapters,
        ad_span_evaluation: AdSpanEvaluation::Evaluated,
        ad_spans: ads,
    })?;
    Ok(QualifiedObservation {
        artifact,
        fingerprint,
    })
}

fn generated_chapters(
    items: Vec<GeneratedChapter>,
    duration: Option<u64>,
) -> Result<Vec<ChapterInput>, ChapterObservationRejection> {
    let mut chapters = Vec::with_capacity(MAX_GENERATED_CHAPTERS);
    let mut previous = None;
    for item in items {
        let title = item.title.trim().to_owned();
        if title.is_empty() {
            continue;
        }
        let start = clamped_milliseconds(item.start, duration)?;
        if previous.is_some_and(|value| start <= value) {
            continue;
        }
        previous = Some(start);
        chapters.push(ChapterInput {
            start_milliseconds: start,
            end_milliseconds: None,
            title,
            summary: trimmed(item.summary),
            image_url: None,
            link_url: None,
            include_in_table_of_contents: true,
            source_episode_id: None,
        });
        if chapters.len() == MAX_GENERATED_CHAPTERS {
            break;
        }
    }
    if chapters.len() < MIN_GENERATED_CHAPTERS {
        return Err(ChapterObservationRejection::NoUsableChapters);
    }
    chapters[0].start_milliseconds = 0;
    Ok(chapters)
}

fn model_ads(
    items: Vec<ModelAd>,
    duration: Option<u64>,
) -> Result<Vec<AdSpanInput>, ChapterObservationRejection> {
    let mut result = Vec::with_capacity(items.len());
    let mut previous_end = None;
    for item in items {
        let (Some(start), Some(end)) = (
            item.start.or(item.start_seconds),
            item.end.or(item.end_seconds),
        ) else {
            continue;
        };
        let start = clamped_milliseconds(start, duration)?;
        let end = clamped_milliseconds(end, duration)?;
        if end <= start || previous_end.is_some_and(|value| start < value) {
            continue;
        }
        result.push(AdSpanInput {
            start_milliseconds: start,
            end_milliseconds: end,
            kind: match item.kind.as_deref() {
                Some("preroll") => ChapterAdKind::Preroll,
                Some("postroll") => ChapterAdKind::Postroll,
                _ => ChapterAdKind::Midroll,
            },
        });
        previous_end = Some(end);
    }
    Ok(result)
}

fn apply_summaries(chapters: &mut [ChapterInput], summaries: Vec<ModelSummary>) {
    for item in summaries {
        let Ok(index) = usize::try_from(item.index) else {
            continue;
        };
        if let Some(chapter) = chapters.get_mut(index) {
            let summary = item.summary.trim();
            if !summary.is_empty() {
                chapter.summary = Some(summary.to_owned());
            }
        }
    }
}

fn ensure_model_counts(
    primary: usize,
    ads: Option<usize>,
) -> Result<(), ChapterObservationRejection> {
    if primary > MAX_CHAPTERS || ads.is_some_and(|count| count > MAX_AD_SPANS) {
        Err(ChapterObservationRejection::CollectionLimit)
    } else {
        Ok(())
    }
}

fn merge_duration(
    observed: Option<u64>,
    base: Option<u64>,
) -> Result<Option<u64>, ChapterObservationRejection> {
    if observed
        .zip(base)
        .is_some_and(|(left, right)| left != right)
    {
        Err(ChapterObservationRejection::InvalidBaseArtifact)
    } else {
        Ok(observed.or(base))
    }
}

fn trimmed(value: Option<String>) -> Option<String> {
    value.and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_owned()))
}
