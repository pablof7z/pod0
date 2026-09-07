use crate::{
    AgentCapabilityExecutionMode, DurableEffectExecution, DurableExternalEffectRequest,
    DurableModelChapterAction, TranscriptCapabilityRequest,
};

impl DurableExternalEffectRequest {
    /// Whether an expired attempt may be leased again without first committing
    /// a recovery transition. Ambiguous submissions are deliberately excluded:
    /// only requests that already name the exact external operation are safe to
    /// reattach directly.
    #[must_use]
    pub fn expired_lease_reclaim_is_exact(&self) -> bool {
        match &self.execution {
            DurableEffectExecution::Transcript { request } => matches!(
                request.capability,
                TranscriptCapabilityRequest::FetchPublisher { .. }
                    | TranscriptCapabilityRequest::RecoverProvider { .. }
            ),
            DurableEffectExecution::ModelChapter { request } => {
                matches!(request.action, DurableModelChapterAction::Recover { .. })
            }
            DurableEffectExecution::AgentModel { .. } => false,
            DurableEffectExecution::AgentCapability { request } => matches!(
                request.capability.execution_mode,
                AgentCapabilityExecutionMode::RecoverExisting
            ),
            _ => true,
        }
    }
}
