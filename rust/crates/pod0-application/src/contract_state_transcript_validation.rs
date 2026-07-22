pub(super) fn transcript_observation_matches(
    request: &crate::TranscriptCapabilityRequest,
    observation: &crate::TranscriptCapabilityObservation,
) -> bool {
    use crate::TranscriptCapabilityObservation as Observation;
    use crate::TranscriptCapabilityRequest as Request;

    if crate::validate_transcript_capability_request(request.clone())
        != crate::TranscriptCapabilityValidation::Accepted
        || crate::validate_transcript_capability_observation(observation.clone())
            != crate::TranscriptCapabilityValidation::Accepted
    {
        return false;
    }
    match (request, observation) {
        (Request::SubmitProvider { .. }, Observation::ProviderAccepted { .. })
        | (Request::RecoverProvider { .. }, Observation::ProviderPending { .. })
        | (_, Observation::Failed { .. } | Observation::Cancelled) => true,
        (
            Request::RecoverProvider {
                external_operation_id,
                ..
            },
            Observation::Completed {
                external_operation_id: observed,
                artifact,
                ..
            },
        ) => {
            observed
                .as_deref()
                .is_none_or(|value| value == external_operation_id)
                && artifact_matches_context(artifact, request.context())
        }
        (
            Request::FetchPublisher { .. }
            | Request::SubmitProvider { .. }
            | Request::TranscribeLocal { .. },
            Observation::Completed { artifact, .. },
        ) => artifact_matches_context(artifact, request.context()),
        _ => false,
    }
}

fn artifact_matches_context(
    artifact: &pod0_domain::TranscriptArtifactInput,
    context: &crate::TranscriptCapabilityContext,
) -> bool {
    artifact.episode_id == context.episode_id
        && artifact.podcast_id == context.podcast_id
        && artifact.source_revision == context.source_revision
}
