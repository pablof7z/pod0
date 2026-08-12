use pod0_application::{
    ActivityDomain, ActivitySubject, AgentArtifactHandoffActivityInput, AgentArtifactMutation,
    AgentToolAction, AgentToolCompletion, InternalCommandKind, RequestDisposition,
    UserArtifactTransition,
    plan_agent_artifact_handoff,
};
use pod0_domain::{CommandId, ContentDigest, NoteAuthor, NoteId};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use super::agent_artifact_support::{hex, require_current_proposal};
use super::application_support::next_core_revision;
use crate::{PendingInternalCommand, StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_agent_note(
    path: &std::path::Path,
    command: PendingInternalCommand,
    observed_at: pod0_domain::UnixTimestampMilliseconds,
) -> Result<NoteId, StorageError> {
    let InternalCommandKind::ExecuteAgentTool { turn_id } = &command.request.kind else {
        return Err(StorageError::InvalidActivity);
    };
    let turn_id = *turn_id;
    if command.request.target != ActivityDomain::UserArtifact
        || command.request.subject != (pod0_application::ActivitySubject::AgentTurn { turn_id })
    {
        return Err(StorageError::InvalidActivity);
    }
    let agent = crate::AgentStore::open(path)?;
    let state = agent
        .turn(turn_id)?
        .ok_or(StorageError::AgentTurnNotFound)?;
    let projection = state.projection();
    let proposal = projection
        .proposal
        .as_ref()
        .ok_or(StorageError::InvalidAgentState)?;
    let action = proposal.action.clone();
    let AgentToolAction::CreateNote { text } = &action else {
        return Err(StorageError::InvalidActivity);
    };
    let note_id = NoteId::from_bytes(proposal.proposal_id.into_bytes());
    let episode_id = command.request.episode_id;
    let fingerprint = fingerprint(command.internal_command_id, note_id, text);
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
            let current = crate::library_store_note_support::collection_revision(transaction)?;
            let committed = next_core_revision(transaction, "read agent note core revision")?;
            plan_agent_artifact_handoff(AgentArtifactHandoffActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                turn_id,
                subject: ActivitySubject::Note { note_id },
                episode_ids: episode_id.into_iter().collect(),
                transition: UserArtifactTransition::NoteChanged,
                completion: AgentToolCompletion::NoteCreated { note_id },
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
            let current = crate::library_store_note_support::collection_revision(transaction)?;
            if current != expected {
                return Err(StorageError::RevisionConflict);
            }
            let (revision, created) =
                crate::library_store_note_create::create_note_in_transaction(
                    transaction,
                    CommandId::from_bytes(command.internal_command_id.into_bytes()),
                    note_id,
                    &fingerprint_text,
                    text,
                    pod0_domain::NoteKind::Free,
                    NoteAuthor::Agent,
                    None,
                    observed_at.value,
                )?;
            if created != note_id || revision != committed {
                return Err(StorageError::RevisionConflict);
            }
            Ok(revision)
        },
    )?;
    Ok(note_id)
}

fn fingerprint(
    command_id: pod0_domain::InternalCommandId,
    note_id: NoteId,
    text: &str,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent/create-note/v1");
    hash.update(command_id.into_bytes());
    hash.update(note_id.into_bytes());
    hash.update(text.as_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}
