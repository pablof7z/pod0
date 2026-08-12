use pod0_application::{
    ActivityDomain, EvidenceAdmissionActivityInput, InternalCommandKind, evidence_phase_command_id,
    plan_evidence_admission,
};
use pod0_domain::{ContentDigest, StateRevision, TranscriptEvidenceArtifact};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{PendingInternalCommand, StorageError, TransitionIngress, TransitionIngressKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceAdmissionCommitInput {
    pub command: PendingInternalCommand,
    pub artifact: TranscriptEvidenceArtifact,
    pub committed_at: pod0_domain::UnixTimestampMilliseconds,
    pub deadline_at: pod0_domain::UnixTimestampMilliseconds,
}

pub(crate) fn commit_evidence_admission(
    path: &std::path::Path,
    input: EvidenceAdmissionCommitInput,
) -> Result<crate::CommitReceipt, StorageError> {
    if input.command.request.kind != InternalCommandKind::BuildTranscriptEvidence
        || input.command.request.target != ActivityDomain::RecallKnowledge
        || input.command.request.episode_id != Some(input.artifact.version.episode_id)
    {
        return Err(StorageError::InvalidActivity);
    }
    let plan = plan_evidence_admission(EvidenceAdmissionActivityInput {
        internal_command_id: input.command.internal_command_id,
        authorizing_activity_id: input.command.authorizing_activity_id,
        correlation_id: input.command.correlation_id,
        episode_id: input.artifact.version.episode_id,
        artifact: input.artifact,
        deadline_at: input.deadline_at,
    })
    .map_err(|_| StorageError::InvalidActivity)?;
    let fingerprint = evidence_fingerprint(&plan);
    TransitionCommit::open(path)?.commit_with(
        TransitionIngress {
            kind: TransitionIngressKind::InternalCommand,
            id: input.command.internal_command_id.into_bytes(),
            fingerprint,
        },
        plan,
        input.committed_at,
        |transaction, expected, artifact| {
            if expected != StateRevision::INITIAL {
                return Err(StorageError::RevisionConflict);
            }
            let generation_id = artifact.generation_id;
            crate::evidence_store_stage::apply_evidence_stage(
                transaction,
                evidence_phase_command_id(generation_id, b"stage"),
                &artifact,
                input.committed_at.value,
            )?;
            crate::evidence_store_mutations::apply_evidence_verification(
                transaction,
                evidence_phase_command_id(generation_id, b"verify"),
                generation_id,
                input.committed_at.value,
            )?;
            crate::evidence_store_mutations::apply_evidence_selection(
                transaction,
                evidence_phase_command_id(generation_id, b"select"),
                artifact.version.episode_id,
                generation_id,
                input.committed_at.value,
            )?;
            Ok(StateRevision::new(1))
        },
    )
}

fn evidence_fingerprint(plan: &pod0_application::EvidenceAdmissionPlan) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/evidence/admission/v1");
    let (_, _, artifact, _, _, _, _) = plan.clone().into_parts();
    hash.update(artifact.generation_id.into_bytes());
    hash.update(artifact.integrity_digest.into_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}
