use pod0_application::{CommandEnvelope, OperationResult, TranscriptCommitReceipt};
use pod0_domain::{StateRevision, TranscriptArtifactInput};

use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn commit_transcript(
        &mut self,
        envelope: &CommandEnvelope,
        expected_selection_revision: StateRevision,
        artifact: TranscriptArtifactInput,
    ) {
        let completed_at_ms = self.now().value;
        let result = self
            .transcript_store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.commit_and_select(
                    envelope.command_id,
                    expected_selection_revision,
                    artifact,
                    completed_at_ms,
                )
            });
        match result {
            Ok(receipt) => {
                if self.reload_listening().is_err() {
                    self.fail(
                        envelope.command_id,
                        pod0_application::CoreFailureCode::StorageUnavailable,
                    );
                    return;
                }
                self.succeed(
                    envelope.command_id,
                    Some(OperationResult::TranscriptCommitted {
                        receipt: TranscriptCommitReceipt {
                            command_id: receipt.command_id,
                            artifact_id: receipt.artifact_id,
                            transcript_version_id: receipt.transcript_version_id,
                            transcript_content_digest: receipt.transcript_content_digest,
                            artifact_integrity_digest: receipt.artifact_integrity_digest,
                            command_fingerprint: receipt.command_fingerprint,
                            selection_revision: receipt.selection_revision,
                            speaker_count: receipt.speaker_count,
                            segment_count: receipt.segment_count,
                            word_count: receipt.word_count,
                        },
                    }),
                )
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }
}
