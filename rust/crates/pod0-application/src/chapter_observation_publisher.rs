use pod0_domain::{
    AdSpanEvaluation, ChapterArtifactInput, ChapterArtifactProvenance, ChapterArtifactSource,
    ChapterInput, MAX_CHAPTERS,
};
use serde::Deserialize;

use crate::chapter_observation_values::{
    ObservationHash, canonicalize, normalize_url, optional_url, payload_digest,
    publisher_content_type, source_revision, strict_milliseconds,
};
use crate::{
    ChapterObservationRejection, PublisherChapterObservation, Qualification, QualifiedObservation,
};

#[derive(Deserialize)]
struct PublisherEnvelope {
    version: Option<String>,
    chapters: Vec<PublisherChapter>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublisherChapter {
    start_time: Option<f64>,
    end_time: Option<f64>,
    title: Option<String>,
    img: Option<String>,
    url: Option<String>,
    toc: Option<bool>,
}

pub(crate) fn qualify(observation: PublisherChapterObservation) -> Qualification {
    if observation.payload.len() > crate::MAX_PUBLISHER_CHAPTER_DOCUMENT_BYTES {
        return Err(ChapterObservationRejection::PayloadTooLarge);
    }
    if payload_digest(&observation.payload) != observation.payload_digest {
        return Err(ChapterObservationRejection::DigestMismatch);
    }

    let source_url = normalize_url(&observation.resolved_source_url, false)?;
    let content_type = publisher_content_type(&observation.content_type)?;
    let payload: PublisherEnvelope = serde_json::from_slice(&observation.payload)
        .map_err(|_| ChapterObservationRejection::MalformedPayload)?;
    let format_version = publisher_format_version(payload.version.as_deref())?;
    if payload.chapters.len() > MAX_CHAPTERS {
        return Err(ChapterObservationRejection::CollectionLimit);
    }

    let mut chapters = Vec::with_capacity(payload.chapters.len());
    for raw in payload.chapters {
        let title = raw.title.unwrap_or_default().trim().to_owned();
        if title.is_empty() {
            continue;
        }
        chapters.push(ChapterInput {
            start_milliseconds: strict_milliseconds(raw.start_time.unwrap_or(0.0))?,
            end_milliseconds: raw.end_time.map(strict_milliseconds).transpose()?,
            title,
            summary: None,
            image_url: optional_url(raw.img, false)?,
            link_url: optional_url(raw.url, false)?,
            include_in_table_of_contents: raw.toc.unwrap_or(true),
            source_episode_id: None,
        });
    }
    if chapters.is_empty() {
        return Err(ChapterObservationRejection::NoUsableChapters);
    }
    chapters.sort_by_key(|chapter| chapter.start_milliseconds);

    let mut hash = ObservationHash::new(b"pod0.publisher-chapter-observation.v1");
    hash.bytes(&observation.episode_id.into_bytes());
    hash.bytes(&observation.podcast_id.into_bytes());
    hash.text(&source_url);
    hash.text(&content_type);
    hash.bytes(&observation.payload_digest.into_bytes());
    hash.u32(format_version);
    hash.optional_u64(observation.duration_milliseconds);
    let fingerprint = hash.finish();
    let artifact = canonicalize(ChapterArtifactInput {
        episode_id: observation.episode_id,
        podcast_id: observation.podcast_id,
        source_revision: source_revision("publisher-json-v1", fingerprint),
        provenance: ChapterArtifactProvenance {
            source: ChapterArtifactSource::Publisher,
            provider: None,
            model: None,
            policy_version: 0,
            source_payload_digest: observation.payload_digest,
            transcript_version_id: None,
            transcript_content_digest: None,
            legacy_import: None,
        },
        generated_at: observation.generated_at,
        duration_milliseconds: observation.duration_milliseconds,
        chapters,
        ad_span_evaluation: AdSpanEvaluation::NotEvaluated,
        ad_spans: Vec::new(),
    })?;
    Ok(QualifiedObservation {
        artifact,
        fingerprint,
    })
}

fn publisher_format_version(version: Option<&str>) -> Result<u32, ChapterObservationRejection> {
    let Some(version) = version else {
        return Ok(1);
    };
    let major = version
        .split('.')
        .next()
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(ChapterObservationRejection::MalformedPayload)?;
    if major == 1 {
        Ok(major)
    } else {
        Err(ChapterObservationRejection::UnsupportedFormat {
            format_version: major,
        })
    }
}
