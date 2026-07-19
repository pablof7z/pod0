use sha2::{Digest as _, Sha256};

use crate::{ContentDigest, StateRevision, TranscriptArtifact};

/// Stable replay identity for the durable commit-and-select transcript command.
#[must_use]
pub fn transcript_command_fingerprint(
    expected_revision: StateRevision,
    artifact: &TranscriptArtifact,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0.commit-transcript.v1\0");
    hash.update(expected_revision.value.to_be_bytes());
    hash.update(artifact.artifact_id.into_bytes());
    hash.update(artifact.integrity_digest.into_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}
