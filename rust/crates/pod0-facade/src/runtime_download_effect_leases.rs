use pod0_application::{ActivitySubject, HostRequestEnvelope};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn download_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let ActivitySubject::Episode { episode_id } = lease.subject else {
            return None;
        };
        if lease.episode_id != Some(episode_id) {
            return None;
        }
        let pod0_application::DurableEffectExecution::Download { request } =
            &lease.request.execution
        else {
            return None;
        };
        (request.episode_id() == episode_id
            && request.not_before == lease.request.not_before
            && request.deadline_at == lease.request.deadline_at)
            .then(|| request.to_host())
    }
}
