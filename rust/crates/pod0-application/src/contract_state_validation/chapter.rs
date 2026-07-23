use crate::{HostObservation, HostRequest};

pub(crate) fn chapter_model_payload_is_bounded(
    request: &HostRequest,
    observation: &HostObservation,
) -> bool {
    if !matches!(
        request,
        HostRequest::ExecuteChapterModel { .. } | HostRequest::RecoverChapterModelOperation { .. }
    ) {
        return true;
    }
    match observation {
        HostObservation::ChapterModelProviderAccepted { update, .. } => {
            !update.provider_operation_id.is_empty()
                && update.provider_operation_id.len() <= 1_024
                && update
                    .provider_status
                    .as_ref()
                    .is_none_or(|value| value.len() <= 1_024)
        }
        HostObservation::ChapterModelCompleted { completion, .. } => {
            !completion.completion.is_empty()
                && !completion.provider.is_empty()
                && completion.provider.len() <= 128
                && !completion.model.is_empty()
                && completion.model.len() <= 256
                && completion
                    .provider_operation_id
                    .as_ref()
                    .is_none_or(|value| !value.is_empty() && value.len() <= 1_024)
                && completion
                    .provider_status
                    .as_ref()
                    .is_none_or(|value| value.len() <= 1_024)
        }
        HostObservation::ChapterModelFailed {
            safe_detail,
            retry_after_milliseconds,
            ..
        } => {
            safe_detail
                .as_ref()
                .is_none_or(|value| value.len() <= 16_384)
                && retry_after_milliseconds.is_none_or(|value| value <= 86_400_000)
        }
        _ => true,
    }
}
