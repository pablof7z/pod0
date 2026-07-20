use pod0_domain::{
    ChapterArtifact, ChapterArtifactError, ChapterArtifactInput, ContentDigest, EpisodeId,
    MAX_CHAPTER_MODEL_BYTES, MAX_PROVENANCE_PROVIDER_BYTES, MAX_SOURCE_REVISION_BYTES,
};
use sha2::{Digest as _, Sha256};
use url::Url;

use crate::ChapterObservationRejection;

pub(crate) struct ObservationHash(Sha256);

impl ObservationHash {
    pub(crate) fn new(domain: &[u8]) -> Self {
        let mut hash = Sha256::new();
        hash.update((domain.len() as u64).to_be_bytes());
        hash.update(domain);
        Self(hash)
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.0.update([value]);
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.0.update(value.to_be_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.0.update(value.to_be_bytes());
    }

    pub(crate) fn bytes(&mut self, value: &[u8]) {
        self.u64(value.len() as u64);
        self.0.update(value);
    }

    pub(crate) fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    pub(crate) fn optional_u64(&mut self, value: Option<u64>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.u64(value);
            }
            None => self.u8(0),
        }
    }

    pub(crate) fn optional_text(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.text(value);
            }
            None => self.u8(0),
        }
    }

    pub(crate) fn optional_episode(&mut self, value: Option<EpisodeId>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.bytes(&value.into_bytes());
            }
            None => self.u8(0),
        }
    }

    pub(crate) fn optional_digest(&mut self, value: Option<ContentDigest>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.bytes(&value.into_bytes());
            }
            None => self.u8(0),
        }
    }

    pub(crate) fn finish(self) -> ContentDigest {
        ContentDigest::from_bytes(self.0.finalize().into())
    }
}

pub(crate) fn payload_digest(bytes: &[u8]) -> ContentDigest {
    ContentDigest::from_bytes(Sha256::digest(bytes).into())
}

pub(crate) fn source_revision(prefix: &str, fingerprint: ContentDigest) -> String {
    let bytes = fingerprint.into_bytes();
    let mut revision = String::with_capacity(prefix.len() + 1 + bytes.len() * 2);
    revision.push_str(prefix);
    revision.push(':');
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut revision, "{byte:02x}").expect("writing to String cannot fail");
    }
    debug_assert!(revision.len() <= MAX_SOURCE_REVISION_BYTES);
    revision
}

pub(crate) fn canonicalize(
    input: ChapterArtifactInput,
) -> Result<ChapterArtifactInput, ChapterObservationRejection> {
    ChapterArtifact::seal(input)
        .map(|artifact| artifact.as_input())
        .map_err(map_artifact_error)
}

pub(crate) fn exact_provider(value: &str) -> Result<String, ChapterObservationRejection> {
    exact_text(value, MAX_PROVENANCE_PROVIDER_BYTES)
}

pub(crate) fn exact_model(value: &str) -> Result<String, ChapterObservationRejection> {
    exact_text(value, MAX_CHAPTER_MODEL_BYTES)
}

pub(crate) fn exact_revision(value: &str) -> Result<String, ChapterObservationRejection> {
    exact_text(value, MAX_SOURCE_REVISION_BYTES)
}

pub(crate) fn strict_milliseconds(seconds: f64) -> Result<u64, ChapterObservationRejection> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(ChapterObservationRejection::InvalidTimestamp);
    }
    let value = (seconds * 1_000.0).round();
    if !value.is_finite() || value >= u64::MAX as f64 {
        return Err(ChapterObservationRejection::InvalidTimestamp);
    }
    Ok(value as u64)
}

pub(crate) fn clamped_milliseconds(
    seconds: f64,
    duration_milliseconds: Option<u64>,
) -> Result<u64, ChapterObservationRejection> {
    if !seconds.is_finite() {
        return Err(ChapterObservationRejection::InvalidTimestamp);
    }
    let lower_bounded = seconds.max(0.0);
    let bounded = duration_milliseconds.map_or(lower_bounded, |duration| {
        lower_bounded.min(duration as f64 / 1_000.0)
    });
    strict_milliseconds(bounded)
}

pub(crate) fn normalize_url(
    value: &str,
    allow_file: bool,
) -> Result<String, ChapterObservationRejection> {
    let parsed = Url::parse(value.trim()).map_err(|_| ChapterObservationRejection::InvalidUrl)?;
    let allowed =
        matches!(parsed.scheme(), "http" | "https") || (allow_file && parsed.scheme() == "file");
    if !allowed || parsed.host().is_none() && parsed.scheme() != "file" {
        return Err(ChapterObservationRejection::InvalidUrl);
    }
    Ok(parsed.to_string())
}

pub(crate) fn optional_url(
    value: Option<String>,
    allow_file: bool,
) -> Result<Option<String>, ChapterObservationRejection> {
    value
        .and_then(|value| (!value.trim().is_empty()).then_some(value))
        .map(|value| normalize_url(&value, allow_file))
        .transpose()
}

pub(crate) fn publisher_content_type(value: &str) -> Result<String, ChapterObservationRejection> {
    let media_type = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if matches!(
        media_type.as_str(),
        "application/json" | "application/json+chapters" | "application/chapters+json"
    ) {
        Ok(media_type)
    } else {
        Err(ChapterObservationRejection::InvalidContentType)
    }
}

fn exact_text(value: &str, limit: usize) -> Result<String, ChapterObservationRejection> {
    if value.is_empty() || value.trim() != value || value.len() > limit {
        Err(ChapterObservationRejection::InvalidProvenance)
    } else {
        Ok(value.to_owned())
    }
}

fn map_artifact_error(value: ChapterArtifactError) -> ChapterObservationRejection {
    use ChapterArtifactError as Domain;
    match value {
        Domain::InvalidMetadata | Domain::InvalidProvenance => {
            ChapterObservationRejection::InvalidProvenance
        }
        Domain::InvalidChapter | Domain::ChaptersOutOfOrder | Domain::ChaptersOverlap => {
            ChapterObservationRejection::InvalidRange
        }
        Domain::InvalidAdSpan | Domain::AdSpansOutOfOrder | Domain::AdSpansOverlap => {
            ChapterObservationRejection::InvalidRange
        }
        Domain::TooManyChapters | Domain::TooManyAdSpans => {
            ChapterObservationRejection::CollectionLimit
        }
        Domain::TextLimit | Domain::ArtifactTooLarge => ChapterObservationRejection::TextLimit,
        Domain::IdentityMismatch => ChapterObservationRejection::InvalidBaseArtifact,
        Domain::UnsupportedSource { .. }
        | Domain::UnsupportedAdKind { .. }
        | Domain::UnsupportedAdEvaluation { .. } => ChapterObservationRejection::InvalidProvenance,
    }
}
