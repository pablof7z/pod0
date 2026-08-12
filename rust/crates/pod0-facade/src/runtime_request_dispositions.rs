use pod0_application::{
    ActivityActor, ActivityOrigin, ActivitySubject, CommandEnvelope, CoreFailureCode,
    RequestDisposition, RequestRejectionReason,
};
use pod0_domain::EpisodeId;

use crate::runtime_command_fingerprint::command_fingerprint_digest;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn reject_application_request(
        &mut self,
        envelope: &CommandEnvelope,
        subject: ActivitySubject,
        episode_id: Option<EpisodeId>,
        reason: RequestRejectionReason,
        failure: CoreFailureCode,
    ) {
        let persisted = self.store.as_ref().is_some_and(|store| {
            store
                .record_request_disposition(
                    envelope.command_id,
                    command_fingerprint_digest(&envelope.command),
                    subject,
                    episode_id,
                    ActivityActor::User,
                    ActivityOrigin::UserInterface,
                    RequestDisposition::Rejected { reason },
                    self.now(),
                )
                .is_ok()
        });
        self.fail(
            envelope.command_id,
            if persisted {
                failure
            } else {
                CoreFailureCode::StorageUnavailable
            },
        );
    }
}
