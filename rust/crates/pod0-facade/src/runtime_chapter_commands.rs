use pod0_application::{ChapterCommitReceipt, CommandEnvelope, OperationResult};
use pod0_domain::{ChapterArtifactInput, StateRevision};

use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn commit_chapter(
        &mut self,
        envelope: &CommandEnvelope,
        expected_selection_revision: StateRevision,
        artifact: ChapterArtifactInput,
    ) {
        let completed_at_ms = self.now().value;
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.commit_and_select_chapter(
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
                    Some(OperationResult::ChapterCommitted {
                        receipt: ChapterCommitReceipt {
                            command_id: receipt.command_id,
                            artifact_id: receipt.artifact_id,
                            content_digest: receipt.content_digest,
                            integrity_digest: receipt.integrity_digest,
                            command_fingerprint: receipt.command_fingerprint,
                            selection_revision: receipt.selection_revision,
                            chapter_count: receipt.chapter_count,
                            ad_span_count: receipt.ad_span_count,
                        },
                    }),
                );
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }
}
