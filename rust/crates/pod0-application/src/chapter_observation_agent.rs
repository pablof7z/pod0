use pod0_domain::{
    AdSpanEvaluation, ChapterArtifactInput, ChapterArtifactProvenance, ChapterArtifactSource,
    ChapterInput, ContentDigest,
};

use crate::chapter_observation_values::{
    ObservationHash, canonicalize, exact_model, exact_provider, exact_revision, optional_url,
    strict_milliseconds,
};
use crate::{
    AgentComposedChapterObservation, CHAPTER_OBSERVATION_POLICY_VERSION,
    ChapterObservationRejection, Qualification, QualifiedObservation,
};

pub(crate) fn qualify(observation: AgentComposedChapterObservation) -> Qualification {
    if observation.items.len() > crate::MAX_AGENT_COMPOSED_CHAPTER_ITEMS {
        return Err(ChapterObservationRejection::CollectionLimit);
    }
    if observation.items.is_empty() {
        return Err(ChapterObservationRejection::NoUsableChapters);
    }
    if observation.policy_version != CHAPTER_OBSERVATION_POLICY_VERSION {
        return Err(ChapterObservationRejection::UnsupportedPolicy {
            policy_version: observation.policy_version,
        });
    }
    if observation.source_payload_digest == ContentDigest::default() {
        return Err(ChapterObservationRejection::InvalidProvenance);
    }
    let composition_revision = exact_revision(&observation.composition_revision)?;
    let provider = observation
        .provider
        .as_deref()
        .map(exact_provider)
        .transpose()?;
    let model = observation.model.as_deref().map(exact_model).transpose()?;

    let mut chapters = Vec::with_capacity(observation.items.len());
    for item in observation.items {
        chapters.push(ChapterInput {
            start_milliseconds: strict_milliseconds(item.start_seconds)?,
            end_milliseconds: Some(strict_milliseconds(item.end_seconds)?),
            title: item.title.trim().to_owned(),
            summary: trimmed(item.summary),
            image_url: optional_url(item.image_url, true)?,
            link_url: optional_url(item.link_url, false)?,
            include_in_table_of_contents: item.include_in_table_of_contents,
            source_episode_id: item.source_episode_id,
        });
    }
    let artifact = canonicalize(ChapterArtifactInput {
        episode_id: observation.episode_id,
        podcast_id: observation.podcast_id,
        source_revision: composition_revision,
        provenance: ChapterArtifactProvenance {
            source: ChapterArtifactSource::AgentComposed,
            provider,
            model,
            policy_version: observation.policy_version,
            source_payload_digest: observation.source_payload_digest,
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

    let mut hash = ObservationHash::new(b"pod0.agent-composed-chapter-observation.v1");
    hash.bytes(&artifact.episode_id.into_bytes());
    hash.bytes(&artifact.podcast_id.into_bytes());
    hash.text(&artifact.source_revision);
    hash.u32(artifact.provenance.policy_version);
    hash.optional_text(artifact.provenance.provider.as_deref());
    hash.optional_text(artifact.provenance.model.as_deref());
    hash.bytes(&artifact.provenance.source_payload_digest.into_bytes());
    hash.optional_u64(artifact.duration_milliseconds);
    hash.u64(artifact.chapters.len() as u64);
    for chapter in &artifact.chapters {
        hash.u64(chapter.start_milliseconds);
        hash.optional_u64(chapter.end_milliseconds);
        hash.text(&chapter.title);
        hash.optional_text(chapter.summary.as_deref());
        hash.optional_text(chapter.image_url.as_deref());
        hash.optional_text(chapter.link_url.as_deref());
        hash.u8(u8::from(chapter.include_in_table_of_contents));
        hash.optional_episode(chapter.source_episode_id);
    }
    Ok(QualifiedObservation {
        artifact,
        fingerprint: hash.finish(),
    })
}

fn trimmed(value: Option<String>) -> Option<String> {
    value.and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_owned()))
}
