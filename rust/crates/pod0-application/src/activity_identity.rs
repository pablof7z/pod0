use pod0_domain::{
    ActivityCorrelationId, ActivityId, ActivityTransactionId, CommandId, EffectAttemptId,
    EffectIntentId, HostRequestId, InternalCommandId, StateRevision, TranscriptWorkflowId,
};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandActivityIdentity {
    command_id: CommandId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptEffectActivityIdentity {
    request_id: HostRequestId,
    workflow_revision: StateRevision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectObservationActivityIdentity {
    attempt_id: EffectAttemptId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptWorkflowActivityIdentity {
    workflow_id: TranscriptWorkflowId,
    workflow_revision: StateRevision,
    phase: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InternalCommandActivityIdentity {
    internal_command_id: InternalCommandId,
}

impl InternalCommandActivityIdentity {
    #[must_use]
    pub const fn new(internal_command_id: InternalCommandId) -> Self {
        Self {
            internal_command_id,
        }
    }

    #[must_use]
    pub fn transaction_id(self) -> ActivityTransactionId {
        ActivityTransactionId::from_bytes(self.derive(1))
    }

    #[must_use]
    pub fn fact_id(self, ordinal: u8) -> ActivityId {
        ActivityId::from_bytes(self.derive(ordinal.saturating_add(16)))
    }

    #[must_use]
    pub fn fact_id_wide(self, ordinal: u32) -> ActivityId {
        let mut hasher = Sha256::new();
        hasher.update(b"pod0/activity/internal-command-fact/v2");
        hasher.update(self.internal_command_id.into_bytes());
        hasher.update(ordinal.to_be_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        ActivityId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
    }

    #[must_use]
    pub fn effect_intent_id(self, ordinal: u8) -> EffectIntentId {
        EffectIntentId::from_bytes(self.derive(ordinal.saturating_add(64)))
    }

    #[must_use]
    pub fn internal_command_id(self, ordinal: u8) -> InternalCommandId {
        InternalCommandId::from_bytes(self.derive(ordinal.saturating_add(96)))
    }

    fn derive(self, discriminator: u8) -> [u8; 16] {
        let mut hasher = Sha256::new();
        hasher.update(b"pod0/activity/internal-command/v1");
        hasher.update([discriminator]);
        hasher.update(self.internal_command_id.into_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        digest[..16].try_into().expect("fixed digest prefix")
    }
}

impl TranscriptWorkflowActivityIdentity {
    pub const FINALIZATION_PHASE: u8 = 1;
    pub const EVIDENCE_COMPLETION_PHASE: u8 = 2;

    #[must_use]
    pub const fn new(
        workflow_id: TranscriptWorkflowId,
        workflow_revision: StateRevision,
        phase: u8,
    ) -> Self {
        Self {
            workflow_id,
            workflow_revision,
            phase,
        }
    }

    #[must_use]
    pub fn transaction_id(self) -> ActivityTransactionId {
        ActivityTransactionId::from_bytes(self.derive(1))
    }

    #[must_use]
    pub fn fact_id(self, ordinal: u8) -> ActivityId {
        ActivityId::from_bytes(self.derive(ordinal.saturating_add(16)))
    }

    #[must_use]
    pub fn internal_command_id(self, ordinal: u8) -> InternalCommandId {
        InternalCommandId::from_bytes(self.derive(ordinal.saturating_add(64)))
    }

    fn derive(self, discriminator: u8) -> [u8; 16] {
        let mut hasher = Sha256::new();
        hasher.update(b"pod0/activity/transcript-workflow/v1");
        hasher.update([self.phase, discriminator]);
        hasher.update(self.workflow_id.into_bytes());
        hasher.update(self.workflow_revision.value.to_be_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        digest[..16].try_into().expect("fixed digest prefix")
    }
}

impl EffectObservationActivityIdentity {
    #[must_use]
    pub const fn new(attempt_id: EffectAttemptId) -> Self {
        Self { attempt_id }
    }

    #[must_use]
    pub fn transaction_id(self) -> ActivityTransactionId {
        ActivityTransactionId::from_bytes(self.derive(1))
    }

    #[must_use]
    pub fn fact_id(self, ordinal: u8) -> ActivityId {
        ActivityId::from_bytes(self.derive(ordinal.saturating_add(16)))
    }

    #[must_use]
    pub fn effect_intent_id(self, ordinal: u8) -> EffectIntentId {
        EffectIntentId::from_bytes(self.derive(ordinal.saturating_add(64)))
    }

    #[must_use]
    pub fn internal_command_id(self, ordinal: u8) -> InternalCommandId {
        InternalCommandId::from_bytes(self.derive(ordinal.saturating_add(96)))
    }

    fn derive(self, discriminator: u8) -> [u8; 16] {
        let mut hasher = Sha256::new();
        hasher.update(b"pod0/activity/effect-observation/v1");
        hasher.update([discriminator]);
        hasher.update(self.attempt_id.into_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        digest[..16].try_into().expect("fixed digest prefix")
    }
}

impl TranscriptEffectActivityIdentity {
    #[must_use]
    pub const fn new(request_id: HostRequestId, workflow_revision: StateRevision) -> Self {
        Self {
            request_id,
            workflow_revision,
        }
    }

    #[must_use]
    pub fn transaction_id(self) -> ActivityTransactionId {
        ActivityTransactionId::from_bytes(self.derive(1))
    }

    #[must_use]
    pub fn correlation_id(self) -> ActivityCorrelationId {
        ActivityCorrelationId::from_bytes(self.derive(2))
    }

    #[must_use]
    pub fn fact_id(self, ordinal: u8) -> ActivityId {
        ActivityId::from_bytes(self.derive(ordinal.saturating_add(16)))
    }

    #[must_use]
    pub fn effect_intent_id(self, ordinal: u8) -> EffectIntentId {
        EffectIntentId::from_bytes(self.derive(ordinal.saturating_add(64)))
    }

    fn derive(self, discriminator: u8) -> [u8; 16] {
        let mut hasher = Sha256::new();
        hasher.update(b"pod0/activity/transcript-effect/v1");
        hasher.update([discriminator]);
        hasher.update(self.request_id.into_bytes());
        hasher.update(self.workflow_revision.value.to_be_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        digest[..16].try_into().expect("fixed digest prefix")
    }
}

impl CommandActivityIdentity {
    #[must_use]
    pub const fn new(command_id: CommandId) -> Self {
        Self { command_id }
    }

    #[must_use]
    pub fn transaction_id(self) -> ActivityTransactionId {
        ActivityTransactionId::from_bytes(self.derive(1))
    }

    #[must_use]
    pub fn correlation_id(self) -> ActivityCorrelationId {
        ActivityCorrelationId::from_bytes(self.derive(2))
    }

    #[must_use]
    pub fn fact_id(self, ordinal: u8) -> ActivityId {
        ActivityId::from_bytes(self.derive(ordinal.saturating_add(16)))
    }

    #[must_use]
    pub fn fact_id_wide(self, ordinal: u32) -> ActivityId {
        let mut hasher = Sha256::new();
        hasher.update(b"pod0/activity/command-fact/v2");
        hasher.update(self.command_id.into_bytes());
        hasher.update(ordinal.to_be_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        ActivityId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
    }

    #[must_use]
    pub fn effect_intent_id(self, ordinal: u8) -> EffectIntentId {
        EffectIntentId::from_bytes(self.derive(ordinal.saturating_add(64)))
    }

    #[must_use]
    pub fn internal_command_id(self, ordinal: u8) -> InternalCommandId {
        InternalCommandId::from_bytes(self.derive(ordinal.saturating_add(96)))
    }

    fn derive(self, discriminator: u8) -> [u8; 16] {
        let mut hasher = Sha256::new();
        hasher.update(b"pod0/activity/v1");
        hasher.update([discriminator]);
        hasher.update(self.command_id.into_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        digest[..16].try_into().expect("fixed digest prefix")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostRequestActivityIdentity {
    request_id: HostRequestId,
}

impl HostRequestActivityIdentity {
    #[must_use]
    pub const fn new(request_id: HostRequestId) -> Self {
        Self { request_id }
    }

    #[must_use]
    pub fn transaction_id(self) -> ActivityTransactionId {
        ActivityTransactionId::from_bytes(self.derive(1))
    }

    #[must_use]
    pub fn correlation_id(self) -> ActivityCorrelationId {
        ActivityCorrelationId::from_bytes(self.derive(2))
    }

    #[must_use]
    pub fn fact_id(self, ordinal: u8) -> ActivityId {
        ActivityId::from_bytes(self.derive(ordinal.saturating_add(16)))
    }

    #[must_use]
    pub fn effect_intent_id(self, ordinal: u8) -> EffectIntentId {
        EffectIntentId::from_bytes(self.derive(ordinal.saturating_add(64)))
    }

    fn derive(self, discriminator: u8) -> [u8; 16] {
        let mut hasher = Sha256::new();
        hasher.update(b"pod0/activity/host-request/v1");
        hasher.update([discriminator]);
        hasher.update(self.request_id.into_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        digest[..16].try_into().expect("fixed digest prefix")
    }
}
