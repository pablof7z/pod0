use pod0_domain::{
    EpisodeRecord, GeneratedArtifactId, PodcastRecord, PublicationArtifactKind, PublicationId,
    PublicationIntent, PublicationMediaEvidence, PublicationRecord, StateRevision,
    UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};
use url::Url;

pub const POD0_PODCAST_SHOW_KIND: u16 = 30_074;
pub const POD0_PODCAST_EPISODE_KIND: u16 = 30_075;
pub const POD0_PUBLICATION_SCHEMA_VERSION: u32 = 1;
pub const MAX_PUBLICATION_FACTS: usize = 128;
pub const MAX_PUBLICATION_DETAIL_BYTES: usize = 512;
pub const MAX_PUBLICATION_URL_BYTES: usize = 8_192;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pod0PublicationDraft {
    pub publication_id: PublicationId,
    pub expected_author_hex: String,
    pub correlation_token: String,
    pub created_at_seconds: u64,
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PublicationStatusObservation {
    pub kind: pod0_domain::PublicationFactKind,
    pub route_id: Option<pod0_domain::PublicationRouteId>,
    pub attempt: Option<u64>,
    pub event_id_hex: Option<String>,
    pub observed_at: Option<UnixTimestampMilliseconds>,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationValidationError {
    UnsupportedKind,
    InvalidSemanticRevision,
    InvalidAuthor,
    InvalidMediaUrl,
    InvalidMediaType,
    InvalidMediaEvidence,
    ArtifactMismatch,
    MissingGeneratedArtifact,
    PayloadTooLarge,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PublicationsProjection {
    pub items: Vec<PublicationRecord>,
    pub operations: Vec<crate::OperationProjection>,
    pub has_more: bool,
}

impl PublicationsProjection {
    pub fn enforce_bounds(&mut self, offset: usize, maximum: usize) {
        self.items.sort_by_key(|item| {
            (
                std::cmp::Reverse(item.updated_at.value),
                std::cmp::Reverse(item.publication_id),
            )
        });
        self.has_more = self.items.len() > offset.saturating_add(maximum);
        self.items = self
            .items
            .iter()
            .skip(offset)
            .take(maximum)
            .cloned()
            .collect();
        if self.operations.len() > crate::MAX_OPERATION_ITEMS {
            self.operations = self
                .operations
                .iter()
                .rev()
                .take(crate::MAX_OPERATION_ITEMS)
                .cloned()
                .collect::<Vec<_>>();
            self.operations.reverse();
        }
        for item in &mut self.items {
            if item.facts.len() > MAX_PUBLICATION_FACTS {
                item.facts = item
                    .facts
                    .iter()
                    .rev()
                    .take(MAX_PUBLICATION_FACTS)
                    .cloned()
                    .collect::<Vec<_>>();
                item.facts.reverse();
            }
        }
    }
}

#[must_use]
pub fn publication_id(artifact_id: GeneratedArtifactId, semantic_revision: u32) -> PublicationId {
    let mut hash = Sha256::new();
    hash.update(b"pod0.publication.generated-episode.v1\0");
    hash.update(artifact_id.into_bytes());
    hash.update(semantic_revision.to_be_bytes());
    PublicationId::from_bytes(hash.finalize()[..16].try_into().expect("digest slice"))
}

#[must_use]
pub fn publication_correlation_token(id: PublicationId) -> String {
    format!("pod0-pub-v1:{}", hex(&id.into_bytes()))
}

pub fn validate_publication_intent(
    intent: &PublicationIntent,
) -> Result<(), PublicationValidationError> {
    if intent.kind != PublicationArtifactKind::GeneratedPodcastEpisode {
        return Err(PublicationValidationError::UnsupportedKind);
    }
    if intent.semantic_revision == 0 {
        return Err(PublicationValidationError::InvalidSemanticRevision);
    }
    if !is_lower_hex(&intent.expected_author_hex, 64) {
        return Err(PublicationValidationError::InvalidAuthor);
    }
    validate_media(&intent.media)
}

fn validate_media(media: &PublicationMediaEvidence) -> Result<(), PublicationValidationError> {
    if media.media_type != "audio/mpeg" {
        return Err(PublicationValidationError::InvalidMediaType);
    }
    if media.byte_count == 0 || media.byte_count > crate::MAX_AGENT_GENERATED_AUDIO_BYTES {
        return Err(PublicationValidationError::InvalidMediaEvidence);
    }
    if !(8..=MAX_PUBLICATION_URL_BYTES).contains(&media.public_url.len()) {
        return Err(PublicationValidationError::InvalidMediaUrl);
    }
    let url =
        Url::parse(&media.public_url).map_err(|_| PublicationValidationError::InvalidMediaUrl)?;
    if url.scheme() != "https" || url.host_str().is_none() || url.username() != "" {
        return Err(PublicationValidationError::InvalidMediaUrl);
    }
    Ok(())
}

pub fn compose_generated_episode_publication(
    publication: &PublicationRecord,
    episode: &EpisodeRecord,
    podcast: &PodcastRecord,
) -> Result<Pod0PublicationDraft, PublicationValidationError> {
    validate_publication_intent(&PublicationIntent {
        artifact_id: publication.artifact_id,
        kind: publication.artifact_kind,
        expected_author_hex: publication.expected_author_hex.clone(),
        semantic_revision: publication.semantic_revision,
        media: publication.media.clone(),
    })?;
    let generated = episode
        .generated_audio
        .as_ref()
        .ok_or(PublicationValidationError::MissingGeneratedArtifact)?;
    if generated.artifact_id != publication.artifact_id
        || generated.media_content_digest != publication.media.content_digest
        || generated.media_byte_count != publication.media.byte_count
        || episode.episode_id != publication.episode_id
        || episode.podcast_id != publication.podcast_id
        || podcast.podcast_id != publication.podcast_id
    {
        return Err(PublicationValidationError::ArtifactMismatch);
    }
    let author = &publication.expected_author_hex;
    let show_d = format!("podcast:guid:{}", hex(&podcast.podcast_id.into_bytes()));
    let mut tags = vec![
        vec![
            "d".into(),
            format!(
                "podcast:item:guid:{}",
                hex(&episode.episode_id.into_bytes())
            ),
        ],
        vec!["title".into(), episode.title.clone()],
        vec![
            "published_at".into(),
            (episode.published_at.value.max(0) / 1_000).to_string(),
        ],
        vec![
            "a".into(),
            format!("{POD0_PODCAST_SHOW_KIND}:{author}:{show_d}"),
        ],
    ];
    if !episode.description.is_empty() {
        tags.push(vec!["summary".into(), episode.description.clone()]);
    }
    if let Some(duration) = episode.duration_milliseconds {
        tags.push(vec!["duration".into(), (duration / 1_000).to_string()]);
    }
    if let Some(image) = episode.image_url.as_ref().or(podcast.image_url.as_ref()) {
        tags.push(vec!["image".into(), image.clone()]);
    }
    let mut imeta = vec![
        "imeta".into(),
        format!("url {}", publication.media.public_url),
        format!("m {}", publication.media.media_type),
        format!("x {}", hex(&publication.media.content_digest.into_bytes())),
        format!("size {}", publication.media.byte_count),
    ];
    if let Some(duration) = episode.duration_milliseconds {
        imeta.push(format!("duration {}", duration / 1_000));
    }
    tags.push(imeta);
    if tags.len() > 32
        || tags.iter().flatten().any(|value| value.len() > 8_192)
        || episode.description.len() > 65_536
    {
        return Err(PublicationValidationError::PayloadTooLarge);
    }
    Ok(Pod0PublicationDraft {
        publication_id: publication.publication_id,
        expected_author_hex: author.clone(),
        correlation_token: publication.correlation_token.clone(),
        created_at_seconds: u64::try_from(publication.prepared_at.value.max(0) / 1_000)
            .unwrap_or_default(),
        kind: POD0_PODCAST_EPISODE_KIND,
        tags,
        content: episode.description.clone(),
    })
}

#[must_use]
pub fn initial_publication_record(
    intent: &PublicationIntent,
    episode: &EpisodeRecord,
    prepared_at: UnixTimestampMilliseconds,
) -> PublicationRecord {
    let id = publication_id(intent.artifact_id, intent.semantic_revision);
    PublicationRecord {
        publication_id: id,
        artifact_id: intent.artifact_id,
        artifact_kind: intent.kind,
        episode_id: episode.episode_id,
        podcast_id: episode.podcast_id,
        semantic_revision: intent.semantic_revision,
        revision: StateRevision::new(1),
        expected_author_hex: intent.expected_author_hex.clone(),
        correlation_token: publication_correlation_token(id),
        media: intent.media.clone(),
        receipt_id: None,
        event_id_hex: None,
        stage: pod0_domain::PublicationStage::Prepared,
        prepared_at,
        updated_at: prepared_at,
        facts: Vec::new(),
    }
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[must_use]
pub fn publication_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex(bytes: &[u8]) -> String {
    publication_hex(bytes)
}

#[cfg(test)]
#[path = "publication_tests.rs"]
mod tests;
