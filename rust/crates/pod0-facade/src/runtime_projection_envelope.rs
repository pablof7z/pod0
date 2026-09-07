use pod0_application::{
    Projection, ProjectionBatchEnvelope, ProjectionBatchRequest, ProjectionEnvelope,
};
use pod0_domain::StateRevision;

use crate::runtime_state::FacadeState;

pub(super) fn projection_envelope(
    state_revision: StateRevision,
    projection: Projection,
) -> ProjectionEnvelope {
    ProjectionEnvelope {
        contract_version: pod0_application::FACADE_CONTRACT_VERSION,
        state_revision,
        content_changed: true,
        projection,
    }
}

impl FacadeState {
    pub(super) fn snapshot_batch(
        &self,
        request: ProjectionBatchRequest,
    ) -> ProjectionBatchEnvelope {
        ProjectionBatchEnvelope {
            contract_version: pod0_application::FACADE_CONTRACT_VERSION,
            state_revision: self.revision,
            projections: request
                .bounded_requests()
                .iter()
                .map(|request| self.snapshot(*request))
                .collect(),
            has_more: request.has_more(),
        }
    }
}
