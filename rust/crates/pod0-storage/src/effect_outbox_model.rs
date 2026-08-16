use pod0_application::{ActivitySubject, DurableExternalEffectRequest};
use pod0_domain::{
    ActivityCorrelationId, ActivityId, EffectAttemptId, EffectIntentId, EffectLeaseId, EpisodeId,
    UnixTimestampMilliseconds,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectOutboxError {
    Storage,
    InvalidLeaseDuration,
    StaleLease,
    InvalidRecord,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectLease {
    pub intent_id: EffectIntentId,
    pub attempt_id: EffectAttemptId,
    pub lease_id: EffectLeaseId,
    pub fence: u64,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub subject: ActivitySubject,
    pub episode_id: Option<EpisodeId>,
    pub request: DurableExternalEffectRequest,
    pub expires_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationEffectLease {
    pub lease: pod0_application::PersistedEffectLeaseIdentity,
    pub draft: pod0_application::Pod0PublicationDraft,
}

impl EffectLease {
    #[must_use]
    pub const fn identity(&self) -> pod0_application::PersistedEffectLeaseIdentity {
        pod0_application::PersistedEffectLeaseIdentity {
            intent_id: self.intent_id,
            authorizing_activity_id: self.authorizing_activity_id,
            correlation_id: self.correlation_id,
            attempt_id: self.attempt_id,
            lease_id: self.lease_id,
            fence: self.fence,
            expires_at: self.expires_at,
        }
    }
}
