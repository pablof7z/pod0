use pod0_domain::EpisodeId;
use rusqlite::OptionalExtension;

use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn activity_page_for_episode(
        &self,
        episode_id: EpisodeId,
        after_sequence: Option<u64>,
        requested_count: u16,
    ) -> Result<crate::ActivityPage, StorageError> {
        crate::ActivityStore::open(self.path())?.page_for_episode(
            episode_id,
            after_sequence,
            requested_count,
        )
    }

    pub fn commit_evidence_admission(
        &self,
        input: crate::transition_commit::EvidenceAdmissionCommitInput,
    ) -> Result<crate::CommitReceipt, StorageError> {
        crate::transition_commit::commit_evidence_admission(self.path(), input)
    }

    pub fn commit_evidence_observation(
        &self,
        input: crate::EvidenceObservationCommitInput,
    ) -> Result<crate::EvidenceObservationCommitOutcome, StorageError> {
        crate::transition_commit::commit_evidence_observation(self.path(), input)
    }

    pub fn completed_recall_observation(
        &self,
        episode_id: EpisodeId,
        generation_id: pod0_domain::EvidenceGenerationId,
    ) -> Result<Option<pod0_application::DurableRecallHostObservation>, StorageError> {
        self.read(|connection| {
            let payload: Option<String> = connection
                .query_row(
                    "SELECT a.observation_json FROM pod0_effect_attempts a \
                     JOIN pod0_effect_intents i ON i.intent_id=a.intent_id \
                     WHERE i.effect_kind_code=3 AND i.episode_id=?1 AND a.state_code=3 \
                     AND a.observation_json IS NOT NULL ORDER BY a.observed_at_ms DESC LIMIT 1",
                    [episode_id.into_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| StorageError::sqlite("read recall observation", error))?;
            payload
                .map(|payload| {
                    let observation: pod0_application::DurableRecallHostObservation =
                        serde_json::from_str(&payload)
                            .map_err(|_| StorageError::InvalidActivity)?;
                    (observation.generation_id == generation_id)
                        .then_some(observation)
                        .ok_or(StorageError::InvalidActivity)
                })
                .transpose()
        })
    }
    pub fn claim_next_effect(
        &self,
        now: pod0_domain::UnixTimestampMilliseconds,
        lease_duration_milliseconds: u32,
    ) -> Result<Option<crate::EffectLease>, crate::EffectOutboxError> {
        crate::EffectOutbox::open(self.path())?
            .claim_next_generated(now, lease_duration_milliseconds)
    }

    pub fn effect_kind(
        &self,
        intent_id: pod0_domain::EffectIntentId,
    ) -> Result<Option<pod0_application::ExternalEffectKind>, crate::EffectOutboxError> {
        crate::EffectOutbox::open(self.path())?.effect_kind(intent_id)
    }

    pub fn cancel_transcript_workflow(
        &self,
        input: crate::TranscriptWorkflowCancellationInput,
    ) -> Result<crate::TranscriptWorkflowRecord, StorageError> {
        crate::transition_commit::commit_transcript_cancellation(self.path(), input)
    }

    pub fn commit_transcript_observation(
        &self,
        input: crate::TranscriptObservationCommitInput,
    ) -> Result<crate::TranscriptObservationCommitOutcome, StorageError> {
        crate::transition_commit::commit_transcript_observation(self.path(), input)
    }

    pub fn transcript_workflow_for_effect_intent(
        &self,
        intent_id: pod0_domain::EffectIntentId,
    ) -> Result<Option<crate::TranscriptWorkflowRecord>, StorageError> {
        self.read(|connection| {
            let episode: Option<Vec<u8>> = connection
                .query_row(
                    "SELECT episode_id FROM pod0_effect_intents WHERE intent_id=?1
                     AND effect_kind_code=7 AND subject_code=6",
                    [intent_id.into_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| StorageError::sqlite("read transcript effect subject", error))?;
            let Some(episode) = episode else {
                return Ok(None);
            };
            let episode_id = EpisodeId::from_bytes(
                episode
                    .try_into()
                    .map_err(|_| StorageError::InvalidActivity)?,
            );
            crate::transcript_workflow::read_workflow(connection, episode_id)
        })
    }
}
