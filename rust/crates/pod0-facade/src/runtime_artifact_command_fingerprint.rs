use pod0_domain::{
    ChapterArtifact, ChapterArtifactInput, StateRevision, TranscriptArtifact,
    TranscriptArtifactInput, transcript_command_fingerprint,
};
use sha2::{Digest, Sha256};

pub(super) fn hash_transcript_commit(
    hash: &mut Sha256,
    expected_selection_revision: StateRevision,
    artifact: &TranscriptArtifactInput,
) {
    hash.update(b"commit-transcript\0");
    hash.update(expected_selection_revision.value.to_be_bytes());
    match TranscriptArtifact::seal(artifact.clone()) {
        Ok(artifact) => hash.update(
            transcript_command_fingerprint(expected_selection_revision, &artifact).into_bytes(),
        ),
        Err(_) => {
            hash.update(b"invalid\0");
            hash.update(artifact.episode_id.into_bytes());
            hash.update(artifact.podcast_id.into_bytes());
            hash.update(artifact.source_revision.as_bytes());
            hash.update(artifact.source_payload_digest.into_bytes());
            hash.update((artifact.speakers.len() as u64).to_be_bytes());
            hash.update((artifact.segments.len() as u64).to_be_bytes());
        }
    }
}

pub(super) fn hash_chapter_commit(
    hash: &mut Sha256,
    expected_selection_revision: StateRevision,
    artifact: &ChapterArtifactInput,
) {
    hash.update(b"commit-chapter\0");
    hash.update(expected_selection_revision.value.to_be_bytes());
    match ChapterArtifact::seal(artifact.clone()) {
        Ok(artifact) => hash.update(
            artifact
                .command_fingerprint(expected_selection_revision)
                .into_bytes(),
        ),
        Err(_) => {
            hash.update(b"invalid\0");
            hash.update(artifact.episode_id.into_bytes());
            hash.update(artifact.podcast_id.into_bytes());
            hash.update(artifact.source_revision.as_bytes());
            hash.update(artifact.provenance.source_payload_digest.into_bytes());
            hash.update((artifact.chapters.len() as u64).to_be_bytes());
            hash.update((artifact.ad_spans.len() as u64).to_be_bytes());
        }
    }
}
