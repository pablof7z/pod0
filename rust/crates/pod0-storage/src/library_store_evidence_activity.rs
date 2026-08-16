use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn commit_evidence_rebuild(
        &self,
        command_id: pod0_domain::CommandId,
        command_fingerprint: pod0_domain::ContentDigest,
        artifact: &pod0_domain::TranscriptEvidenceArtifact,
        effect: Option<pod0_application::DurableEvidenceEmbeddingEffectRequest>,
        committed_at_ms: i64,
    ) -> Result<pod0_domain::StateRevision, StorageError> {
        crate::transition_commit::commit_evidence_rebuild(
            self.path(),
            command_id,
            command_fingerprint,
            artifact,
            effect,
            committed_at_ms,
        )
    }
}
