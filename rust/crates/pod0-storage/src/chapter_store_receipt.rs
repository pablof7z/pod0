use pod0_domain::{ChapterArtifact, ChapterArtifactId, CommandId, ContentDigest, StateRevision};

use crate::{ChapterCommitStorageReceipt, StorageError};

pub(crate) fn chapter_commit_receipt(
    command_id: CommandId,
    command_fingerprint: ContentDigest,
    previous_artifact_id: Option<ChapterArtifactId>,
    selection_revision: StateRevision,
    already_selected: bool,
    artifact: &ChapterArtifact,
) -> Result<ChapterCommitStorageReceipt, StorageError> {
    Ok(ChapterCommitStorageReceipt {
        command_id,
        artifact_id: artifact.artifact_id,
        content_digest: artifact.content_digest,
        integrity_digest: artifact.integrity_digest,
        command_fingerprint,
        previous_artifact_id,
        selection_revision,
        chapter_count: u32::try_from(artifact.chapters.len())
            .map_err(|_| StorageError::InvalidChapterArtifact)?,
        ad_span_count: u32::try_from(artifact.ad_spans.len())
            .map_err(|_| StorageError::InvalidChapterArtifact)?,
        already_selected,
    })
}
