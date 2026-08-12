use pod0_application::{
    CommandEnvelope, CoreFailureCode, RequestDisposition, TranscriptWorkflowOrigin,
};
use pod0_domain::EpisodeId;
use pod0_storage::LibraryStore;

use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_transcript_disposition(
        &mut self,
        store: &LibraryStore,
        envelope: &CommandEnvelope,
        fingerprint: pod0_domain::ContentDigest,
        episode_id: EpisodeId,
        origin: TranscriptWorkflowOrigin,
        disposition: RequestDisposition,
    ) -> bool {
        match store.record_transcript_request_disposition(
            envelope.command_id,
            fingerprint,
            episode_id,
            self.revision,
            origin,
            disposition,
            self.now(),
        ) {
            Ok(_) => true,
            Err(error) => {
                self.fail(envelope.command_id, storage_failure(error));
                false
            }
        }
    }

    pub(super) fn authoritative_transcript_workflow_store(
        &mut self,
        envelope: &CommandEnvelope,
    ) -> Option<LibraryStore> {
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return None;
        };
        match store.transcript_workflow_authority() {
            Ok(state) if state.is_authoritative() => Some(store),
            Ok(_) => {
                self.fail(envelope.command_id, CoreFailureCode::HostUnavailable);
                None
            }
            Err(error) => {
                self.fail(envelope.command_id, storage_failure(error));
                None
            }
        }
    }
}
