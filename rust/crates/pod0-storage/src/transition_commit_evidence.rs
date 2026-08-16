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
    pub effect: Option<pod0_application::DurableEvidenceEmbeddingEffectRequest>,
    pub committed_at: pod0_domain::UnixTimestampMilliseconds,
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
    let fingerprint = evidence_fingerprint(&input);
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::InternalCommand,
            id: input.command.internal_command_id.into_bytes(),
            fingerprint,
        },
        input.committed_at,
        |_| {
            plan_evidence_admission(EvidenceAdmissionActivityInput {
                internal_command_id: input.command.internal_command_id,
                authorizing_activity_id: input.command.authorizing_activity_id,
                correlation_id: input.command.correlation_id,
                episode_id: input.artifact.version.episode_id,
                artifact: input.artifact.clone(),
                effect: input.effect.clone(),
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
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

fn evidence_fingerprint(input: &EvidenceAdmissionCommitInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/evidence/admission/v1");
    hash.update(input.artifact.generation_id.into_bytes());
    hash.update(input.artifact.integrity_digest.into_bytes());
    hash.update(serde_json::to_vec(&input.effect).expect("typed evidence effect"));
    ContentDigest::from_bytes(hash.finalize().into())
}
