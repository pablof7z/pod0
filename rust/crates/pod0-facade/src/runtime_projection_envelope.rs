use pod0_application::{Projection, ProjectionEnvelope};
use pod0_domain::StateRevision;

pub(super) fn projection_envelope(
    state_revision: StateRevision,
    projection: Projection,
) -> ProjectionEnvelope {
    ProjectionEnvelope {
        contract_version: pod0_application::FACADE_CONTRACT_VERSION,
        state_revision,
        projection,
    }
}
