use std::path::{Path, PathBuf};

use pod0_application::{
    ActivityFactDraft, ActivitySubject, DurableExternalEffectRequest,
    DurableInternalCommandRequest, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
};
use pod0_domain::{ActivityTransactionId, StateRevision, UnixTimestampMilliseconds};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use crate::StorageError;
use crate::activity_store::append_activity_facts;
use crate::migration_db::{
    configure, open_connection, user_version, validate_current_database_identity,
};
use crate::transition_commit_model::{CommitReceipt, TransitionIngress, TransitionIngressKind};

pub(crate) struct JournalAppendAuthority(());

#[path = "transition_commit_legacy_effect_recovery.rs"]
mod legacy_effect_recovery;
pub(crate) use legacy_effect_recovery::append_v40_legacy_recovery_facts;

#[path = "transition_commit_application_support.rs"]
mod application_support;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommitFaultPoint {
    BeforeMutation,
    AfterMutation,
    AfterFacts,
    AfterEffectIntents,
    AfterInternalCommands,
    AfterReceipt,
}

#[derive(Clone, Debug)]
pub(crate) struct TransitionCommit {
    path: PathBuf,
}

impl TransitionCommit {
    pub(super) fn append_migration_facts(
        transaction: &Transaction<'_>,
        facts: &pod0_application::NonEmptyActivityFacts,
        committed_at: UnixTimestampMilliseconds,
    ) -> Result<(), StorageError> {
        append_activity_facts(
            &JournalAppendAuthority(()),
            transaction,
            facts,
            committed_at,
        )
        .map(|_| ())
    }

    pub(crate) fn open(path: &Path) -> Result<Self, StorageError> {
        let connection = open_connection(path, true)?;
        validate_current_database_identity(&connection, user_version(&connection)?)?;
        Ok(Self { path: path.into() })
    }

    #[cfg(test)]
    pub(crate) fn commit_no_state_change(
        &self,
        ingress: TransitionIngress,
        plan: TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>,
        committed_at: UnixTimestampMilliseconds,
    ) -> Result<CommitReceipt, StorageError> {
        self.commit_with(ingress, plan, committed_at, |_, expected, ()| Ok(expected))
    }

    #[cfg(test)]
    pub(super) fn commit_with<M>(
        &self,
        ingress: TransitionIngress,
        plan: TransitionPlan<M, DurableExternalEffectRequest, DurableInternalCommandRequest>,
        committed_at: UnixTimestampMilliseconds,
        mutate: impl FnOnce(&Transaction<'_>, StateRevision, M) -> Result<StateRevision, StorageError>,
    ) -> Result<CommitReceipt, StorageError> {
        self.commit_with_fault(ingress, plan, committed_at, mutate, |_| Ok(()))
    }

    #[cfg(test)]
    fn commit_with_fault<M>(
        &self,
        ingress: TransitionIngress,
        plan: TransitionPlan<M, DurableExternalEffectRequest, DurableInternalCommandRequest>,
        committed_at: UnixTimestampMilliseconds,
        mutate: impl FnOnce(&Transaction<'_>, StateRevision, M) -> Result<StateRevision, StorageError>,
        mut fault: impl FnMut(CommitFaultPoint) -> Result<(), StorageError>,
    ) -> Result<CommitReceipt, StorageError> {
        self.commit_with_hooks_and_fault(
            ingress,
            plan,
            committed_at,
            |_| Ok(()),
            mutate,
            |_| Ok(()),
            &mut fault,
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn commit_with_hooks_and_fault<M>(
        &self,
        ingress: TransitionIngress,
        plan: TransitionPlan<M, DurableExternalEffectRequest, DurableInternalCommandRequest>,
        committed_at: UnixTimestampMilliseconds,
        before_mutation: impl FnOnce(&Transaction<'_>) -> Result<(), StorageError>,
        mutate: impl FnOnce(&Transaction<'_>, StateRevision, M) -> Result<StateRevision, StorageError>,
        after_activity: impl FnOnce(&Transaction<'_>) -> Result<(), StorageError>,
        fault: impl FnMut(CommitFaultPoint) -> Result<(), StorageError>,
    ) -> Result<CommitReceipt, StorageError> {
        self.commit_planned_with_hooks_and_fault(
            |_| Ok(ingress),
            committed_at,
            |_| Ok(plan),
            before_mutation,
            mutate,
            after_activity,
            fault,
        )
    }
}

include!("transition_commit_planned.rs");

include!("transition_commit_write.rs");
include!("transition_commit_values.rs");

#[path = "transition_commit_library.rs"]
mod library;
pub(crate) use library::commit_episode_starred;

include!("transition_commit_user_artifact_modules.rs");

#[path = "transition_commit_note_cutover.rs"]
mod note_cutover;
pub(crate) use note_cutover::commit_note_cutover;
#[path = "transition_commit_clip_cutover.rs"]
mod clip_cutover;
pub(crate) use clip_cutover::commit_clip_cutover;
#[path = "transition_commit_memory_cutover.rs"]
mod memory_cutover;
pub(crate) use memory_cutover::commit_memory_cutover;

#[path = "transition_commit_speaker.rs"]
mod speaker;
pub(crate) use speaker::{commit_speaker_assignment, commit_speaker_create, commit_speaker_rename};

#[path = "transition_commit_request_disposition.rs"]
mod request_disposition;
pub(crate) use request_disposition::commit_request_disposition;
#[path = "transition_commit_recall_configuration.rs"]
mod recall_configuration;
pub(crate) use recall_configuration::{
    commit_recall_configuration_import, commit_recall_configuration_set,
};
#[path = "transition_commit_workflow_configuration.rs"]
mod workflow_configuration;
pub(crate) use workflow_configuration::{
    commit_workflow_capabilities, commit_workflow_configuration_import,
    commit_workflow_configuration_set,
};
#[path = "transition_commit_workflow_reconcile.rs"]
mod workflow_reconcile;
pub(crate) use workflow_reconcile::{
    commit_workflow_reconcile, commit_workflow_reconcile_from_internal_command,
};
#[path = "transition_commit_internal_disposition.rs"]
mod internal_disposition;
pub(crate) use internal_disposition::commit_internal_command_disposition;
#[path = "transition_commit_recall_cutover.rs"]
mod recall_cutover;
pub(crate) use recall_cutover::commit_recall_index_cutover_start;
#[path = "transition_commit_recall_cutover_observation.rs"]
mod recall_cutover_observation;
pub(crate) use recall_cutover_observation::{
    commit_recall_index_cutover_finalize, commit_recall_index_cutover_observation,
};
#[path = "transition_commit_recall_query.rs"]
mod recall_query;
pub(crate) use recall_query::commit_recall_query_start;
#[path = "transition_commit_recall_query_observation.rs"]
mod recall_query_observation;
pub(crate) use recall_query_observation::commit_recall_query_observation;
#[path = "transition_commit_evidence_rebuild.rs"]
mod evidence_rebuild;
pub(crate) use evidence_rebuild::commit_evidence_rebuild;
#[cfg(test)]
pub(crate) use evidence_rebuild::commit_evidence_rebuild_with_observer;

include!("transition_commit_agent_modules.rs");

#[path = "transition_commit_agent_history_cutover.rs"]
mod agent_history_cutover;
pub(crate) use agent_history_cutover::{
    commit_agent_history_cutover_authority, commit_agent_history_cutover_discard,
    commit_agent_history_cutover_stage, commit_agent_history_cutover_verify,
};

#[path = "transition_commit_publication.rs"]
mod publication;
pub(crate) use publication::commit_publication_prepare;
#[path = "transition_commit_publication_observation.rs"]
mod publication_observation;
pub(crate) use publication_observation::{
    commit_publication_observation, commit_publication_receipt,
};

#[path = "transition_commit_scheduled_agent_observation.rs"]
mod scheduled_agent_observation;
pub(crate) use scheduled_agent_observation::commit_scheduled_agent_observation;
#[path = "transition_commit_scheduled_agent_reconcile.rs"]
mod scheduled_agent_reconcile;
pub(crate) use scheduled_agent_reconcile::commit_scheduled_agent_reconcile;
#[path = "transition_commit_scheduled_agent_internal.rs"]
mod scheduled_agent_internal;
pub(crate) use scheduled_agent_internal::commit_scheduled_agent_internal_reconcile;
#[path = "transition_commit_scheduled_agent_commands.rs"]
mod scheduled_agent_commands;
#[path = "transition_commit_scheduled_agent_effects.rs"]
mod scheduled_agent_effects;
pub(crate) use scheduled_agent_commands::{
    commit_scheduled_task_ensure, commit_scheduled_task_remove, commit_scheduled_task_update,
};
#[path = "transition_commit_scheduled_agent_actions.rs"]
mod scheduled_agent_actions;
pub(crate) use scheduled_agent_actions::{
    commit_scheduled_occurrence_cancel, commit_scheduled_occurrence_retry,
};
#[path = "transition_commit_scheduled_agent_cutover.rs"]
mod scheduled_agent_cutover;
pub(crate) use scheduled_agent_cutover::{
    commit_scheduled_agent_cutover_authority, commit_scheduled_agent_cutover_discard,
    commit_scheduled_agent_cutover_stage, commit_scheduled_agent_cutover_verify,
};

#[path = "transition_commit_playback.rs"]
mod playback;
#[path = "transition_commit_playback_effects.rs"]
mod playback_effects;
pub(crate) use playback::commit_playback_mutation;
#[path = "transition_commit_playback_observation.rs"]
mod playback_observation;
#[path = "transition_commit_playback_observation_fingerprint.rs"]
mod playback_observation_fingerprint;
pub use playback_observation::PlaybackObservationCommitInput;
pub use playback_observation::PlaybackObservationReaction;
#[path = "transition_commit_cancellation.rs"]
mod cancellation;
#[path = "transition_commit_cancellation_observation.rs"]
mod cancellation_observation;
pub use cancellation_observation::{
    CancellationObservationCommitInput, CancellationObservationCommitOutcome,
};
#[path = "transition_commit_reset_listening.rs"]
mod reset_listening;

#[path = "transition_commit_download.rs"]
mod download;
pub(crate) use download::{commit_download_admission, commit_download_internal_admission};
#[path = "transition_commit_download_disposition.rs"]
mod download_disposition;
pub(crate) use download_disposition::{commit_download_internal_disposition, commit_download_noop};
#[path = "transition_commit_download_control.rs"]
mod download_control;
pub(crate) use download_control::{commit_download_cancel, commit_download_remove};
#[path = "transition_commit_download_cutover.rs"]
mod download_cutover;
pub(crate) use download_cutover::commit_download_cutover;
#[path = "transition_commit_download_artifact_recovery.rs"]
pub(crate) mod download_artifact_recovery;
#[path = "transition_commit_download_environment.rs"]
mod download_environment;
#[path = "transition_commit_download_finalization.rs"]
mod download_finalization;
#[path = "transition_commit_download_finalization_apply.rs"]
mod download_finalization_apply;
#[path = "transition_commit_download_observation.rs"]
mod download_observation;
#[path = "transition_commit_download_observation_fingerprint.rs"]
mod download_observation_fingerprint;
#[path = "transition_commit_download_recovery.rs"]
mod download_recovery;
pub(crate) use download_artifact_recovery::{
    DownloadArtifactRecovery, commit_download_artifact_recovery,
};
pub(crate) use download_environment::commit_download_environment;
pub(crate) use download_recovery::commit_waiting_download_reconciliation;

include!("transition_commit_knowledge_modules.rs");

#[cfg(test)]
#[path = "transition_commit_tests.rs"]
mod tests;
