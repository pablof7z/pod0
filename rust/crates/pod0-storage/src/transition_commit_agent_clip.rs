use pod0_application::{
    ActivityDomain, ActivitySubject, AgentArtifactHandoffActivityInput, AgentArtifactMutation,
    AgentToolAction, AgentToolCompletion, InternalCommandKind, RequestDisposition,
    UserArtifactTransition,
    plan_agent_artifact_handoff,
};
use pod0_domain::{ClipId, ClipSource, CommandId, ContentDigest};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use super::agent_artifact_support::{hex, require_current_proposal};
use super::application_support::next_core_revision;
use crate::{PendingInternalCommand, StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_agent_clip(
    path: &std::path::Path,
    command: PendingInternalCommand,
    observed_at: pod0_domain::UnixTimestampMilliseconds,
) -> Result<ClipId, StorageError> {
    let InternalCommandKind::ExecuteAgentTool { turn_id } = &command.request.kind else {
        return Err(StorageError::InvalidActivity);
    };
    let turn_id = *turn_id;
    if command.request.target != ActivityDomain::UserArtifact
        || command.request.subject != (ActivitySubject::AgentTurn { turn_id })
    {
        return Err(StorageError::InvalidActivity);
    }
    let agent = crate::AgentStore::open(path)?;
    let state = agent.turn(turn_id)?.ok_or(StorageError::AgentTurnNotFound)?;
    let projection = state.projection();
    let proposal = projection.proposal.as_ref().ok_or(StorageError::InvalidAgentState)?;
    let action = proposal.action.clone();
    let AgentToolAction::CreateClip {
        episode_id,
        podcast_id,
        start_milliseconds,
        end_milliseconds,
        caption,
        frozen_transcript_text,
    } = &action
    else {
        return Err(StorageError::InvalidActivity);
    };
    let clip_id = ClipId::from_bytes(proposal.proposal_id.into_bytes());
    let fingerprint = fingerprint(command.internal_command_id, clip_id, &action);
    let fingerprint_text = hex(fingerprint.into_bytes());
    let ingress = TransitionIngress {
            kind: TransitionIngressKind::InternalCommand,
            id: command.internal_command_id.into_bytes(),
            fingerprint,
        };
    TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        observed_at,
        |transaction| {
            require_current_proposal(transaction, turn_id, proposal.proposal_id, &action)?;
            let current = crate::library_store_clip_support::collection_revision(transaction)?;
            let committed = next_core_revision(transaction, "read agent clip core revision")?;
            plan_agent_artifact_handoff(AgentArtifactHandoffActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                turn_id,
                subject: ActivitySubject::Clip { clip_id },
                episode_ids: vec![*episode_id],
                transition: UserArtifactTransition::ClipChanged,
                completion: AgentToolCompletion::ClipCreated { clip_id },
                current_revision: current,
                committed_revision: committed,
                disposition: RequestDisposition::Accepted,
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, committed)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, (mutation, committed)| {
            if mutation != AgentArtifactMutation::Apply {
                return Err(StorageError::InvalidActivity);
            }
            if crate::library_store_clip_support::collection_revision(transaction)? != expected {
                return Err(StorageError::RevisionConflict);
            }
            let revision = crate::library_store_clip_create::create_clip_in_transaction(
                transaction,
                CommandId::from_bytes(command.internal_command_id.into_bytes()),
                &fingerprint_text,
                clip_id,
                *episode_id,
                *podcast_id,
                *start_milliseconds,
                *end_milliseconds,
                caption.as_deref(),
                None,
                frozen_transcript_text,
                ClipSource::Agent,
                observed_at.value,
            )?;
            if revision != committed {
                return Err(StorageError::RevisionConflict);
            }
            Ok(revision)
        },
    )?;
    Ok(clip_id)
}

fn fingerprint(
    command_id: pod0_domain::InternalCommandId,
    clip_id: ClipId,
    action: &AgentToolAction,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent/create-clip/v1");
    hash.update(command_id.into_bytes());
    hash.update(clip_id.into_bytes());
    hash.update(serde_json::to_vec(action).expect("typed agent action"));
    ContentDigest::from_bytes(hash.finalize().into())
}
