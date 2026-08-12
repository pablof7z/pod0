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
pub struct TransitionCommit {
    path: PathBuf,
}

impl TransitionCommit {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let connection = open_connection(path, true)?;
        validate_current_database_identity(&connection, user_version(&connection)?)?;
        Ok(Self { path: path.into() })
    }

    pub fn commit_no_state_change(
        &self,
        ingress: TransitionIngress,
        plan: TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>,
        committed_at: UnixTimestampMilliseconds,
    ) -> Result<CommitReceipt, StorageError> {
        self.commit_with(ingress, plan, committed_at, |_, expected, ()| Ok(expected))
    }

    pub(super) fn commit_with<M>(
        &self,
        ingress: TransitionIngress,
        plan: TransitionPlan<M, DurableExternalEffectRequest, DurableInternalCommandRequest>,
        committed_at: UnixTimestampMilliseconds,
        mutate: impl FnOnce(&Transaction<'_>, StateRevision, M) -> Result<StateRevision, StorageError>,
    ) -> Result<CommitReceipt, StorageError> {
        self.commit_with_fault(ingress, plan, committed_at, mutate, |_| Ok(()))
    }

    fn commit_with_transaction_hooks<M>(
        &self,
        ingress: TransitionIngress,
        plan: TransitionPlan<M, DurableExternalEffectRequest, DurableInternalCommandRequest>,
        committed_at: UnixTimestampMilliseconds,
        before_mutation: impl FnOnce(&Transaction<'_>) -> Result<(), StorageError>,
        mutate: impl FnOnce(&Transaction<'_>, StateRevision, M) -> Result<StateRevision, StorageError>,
        after_activity: impl FnOnce(&Transaction<'_>) -> Result<(), StorageError>,
    ) -> Result<CommitReceipt, StorageError> {
        self.commit_with_hooks_and_fault(
            ingress,
            plan,
            committed_at,
            before_mutation,
            mutate,
            after_activity,
            |_| Ok(()),
        )
    }

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

#[path = "transition_commit_request_disposition.rs"]
mod request_disposition;
pub(crate) use request_disposition::commit_request_disposition;

include!("transition_commit_agent_modules.rs");

#[path = "transition_commit_playback.rs"]
mod playback;
pub(crate) use playback::commit_playback_mutation;

#[path = "transition_commit_download.rs"]
mod download;
pub(crate) use download::{commit_download_admission, commit_download_internal_admission};
#[path = "transition_commit_download_disposition.rs"]
mod download_disposition;
pub(crate) use download_disposition::{commit_download_internal_disposition, commit_download_noop};
#[path = "transition_commit_download_control.rs"]
mod download_control;
pub(crate) use download_control::{commit_download_cancel, commit_download_remove};
#[path = "transition_commit_download_environment.rs"]
mod download_environment;
pub(crate) use download_environment::commit_download_environment;

#[path = "transition_commit_chapter_artifact.rs"]
mod chapter_artifact;
pub(crate) use chapter_artifact::commit_chapter_artifact;

#[path = "transition_commit_evidence.rs"]
mod evidence;
pub use evidence::EvidenceAdmissionCommitInput;
pub(crate) use evidence::commit_evidence_admission;

#[path = "transition_commit_evidence_observation.rs"]
mod evidence_observation;
pub(crate) use evidence_observation::commit_evidence_observation;
pub use evidence_observation::{EvidenceObservationCommitInput, EvidenceObservationCommitOutcome};

#[path = "transition_commit_transcript.rs"]
mod transcript;
pub(crate) use transcript::commit_transcript_publisher_effect;

#[path = "transition_commit_transcript_admission.rs"]
mod transcript_admission;
#[path = "transition_commit_transcript_artifact.rs"]
mod transcript_artifact;
#[path = "transition_commit_transcript_cancellation.rs"]
mod transcript_cancellation;
#[path = "transition_commit_transcript_finalization.rs"]
mod transcript_finalization;
#[path = "transition_commit_transcript_internal_disposition.rs"]
mod transcript_internal_disposition;
#[path = "transition_commit_transcript_observation.rs"]
mod transcript_observation;
#[path = "transition_commit_transcript_observation_apply.rs"]
mod transcript_observation_apply;
pub(crate) use transcript::{commit_transcript_recovery_effect, commit_transcript_submission};
pub(crate) use transcript_admission::{
    commit_transcript_admission, commit_transcript_internal_admission,
    transcript_admission_fingerprint,
};
pub(crate) use transcript_artifact::commit_transcript_artifact;
pub(crate) use transcript_cancellation::commit_transcript_cancellation;
pub(crate) use transcript_finalization::commit_transcript_evidence_completion;
pub(crate) use transcript_finalization::commit_transcript_finalization;
pub(crate) use transcript_internal_disposition::commit_transcript_internal_disposition;
pub(crate) use transcript_observation::commit_transcript_observation;

#[cfg(test)]
#[path = "transition_commit_tests.rs"]
mod tests;
