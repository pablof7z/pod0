use pod0_domain::{
    CommandId, StateRevision, TranscriptArtifact, TranscriptArtifactInput,
    transcript_command_fingerprint,
};

use crate::StorageError;
use crate::transcript_store::TranscriptStore;
use crate::transcript_store_codec::artifact_error;
use crate::transcript_store_model::TranscriptCommitStorageReceipt;
#[cfg(test)]
use crate::transcript_store_write::commit_and_select_transcript_in_transaction;

impl TranscriptStore {
    pub fn commit_and_select(
        &self,
        command_id: CommandId,
        expected_selection_revision: StateRevision,
        input: TranscriptArtifactInput,
        completed_at_ms: i64,
    ) -> Result<TranscriptCommitStorageReceipt, StorageError> {
        let artifact = TranscriptArtifact::seal(input.clone()).map_err(artifact_error)?;
        let fingerprint = transcript_command_fingerprint(expected_selection_revision, &artifact);
        crate::transition_commit::commit_transcript_artifact(
            self.path(),
            command_id,
            fingerprint,
            expected_selection_revision,
            input,
            completed_at_ms,
        )
    }

    pub fn commit_and_select_with_activity_fingerprint(
        &self,
        command_id: CommandId,
        activity_fingerprint: pod0_domain::ContentDigest,
        expected_selection_revision: StateRevision,
        input: TranscriptArtifactInput,
        completed_at_ms: i64,
    ) -> Result<TranscriptCommitStorageReceipt, StorageError> {
        crate::transition_commit::commit_transcript_artifact(
            self.path(),
            command_id,
            activity_fingerprint,
            expected_selection_revision,
            input,
            completed_at_ms,
        )
    }

    #[cfg(test)]
    pub(crate) fn commit_and_select_with_observer<F>(
        &self,
        command_id: CommandId,
        expected_selection_revision: StateRevision,
        input: TranscriptArtifactInput,
        completed_at_ms: i64,
        before_commit: F,
    ) -> Result<TranscriptCommitStorageReceipt, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        let artifact = TranscriptArtifact::seal(input).map_err(artifact_error)?;
        if completed_at_ms < 0 || artifact.generated_at.value < 0 {
            return Err(StorageError::InvalidTranscriptArtifact);
        }
        self.write(|transaction| {
            let receipt = commit_and_select_transcript_in_transaction(
                transaction,
                command_id,
                expected_selection_revision,
                &artifact,
                completed_at_ms,
            )?;
            before_commit()?;
            Ok(receipt)
        })
    }
}
