use super::TranscriptWorkflowRecord;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptObservationCommitInput {
    pub lease: pod0_application::PersistedEffectLeaseIdentity,
    pub observation: pod0_application::DurableTranscriptHostObservation,
    pub decision: pod0_application::TranscriptObservationDecision,
    pub committed_at: pod0_domain::UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptObservationCommitOutcome {
    pub workflow: TranscriptWorkflowRecord,
    pub replayed: bool,
}
