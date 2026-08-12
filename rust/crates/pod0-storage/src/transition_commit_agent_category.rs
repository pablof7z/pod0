use pod0_application::{
    ActivityDomain, ActivitySubject, AgentArtifactHandoffActivityInput, AgentArtifactMutation,
    AgentToolAction, AgentToolCompletion, InternalCommandKind, RequestDisposition,
    RequestRejectionReason, UserArtifactTransition, plan_agent_artifact_handoff,
    validate_agent_action,
};
use pod0_domain::{
    CategoryId, CategoryItemKind, CategoryOrigin, CommandId, ContentDigest, EpisodeId,
    LibraryItemId, StateRevision,
};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use super::agent_artifact_support::{hex, require_current_proposal};
use super::application_support::next_core_revision;
use crate::{CategoryEdit, PendingInternalCommand, StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_agent_category(
    path: &std::path::Path,
    command: PendingInternalCommand,
    observed_at: pod0_domain::UnixTimestampMilliseconds,
) -> Result<CategoryId, StorageError> {
    let InternalCommandKind::ExecuteAgentTool { turn_id } = command.request.kind else {
        return Err(StorageError::InvalidActivity);
    };
    if command.request.target != ActivityDomain::UserArtifact
        || command.request.subject != (ActivitySubject::AgentTurn { turn_id })
    {
        return Err(StorageError::InvalidActivity);
    }
    let state = crate::AgentStore::open(path)?
        .turn(turn_id)?
        .ok_or(StorageError::AgentTurnNotFound)?;
    let proposal = state
        .projection()
        .proposal
        .ok_or(StorageError::InvalidAgentState)?;
    let action = proposal.action;
    if !matches!(action, AgentToolAction::WriteCategory { .. } | AgentToolAction::TagItems { .. }) {
        return Err(StorageError::InvalidActivity);
    }
    let category_id = match &action {
        AgentToolAction::WriteCategory { category_id, .. } => category_id
            .unwrap_or_else(|| CategoryId::from_bytes(proposal.proposal_id.into_bytes())),
        AgentToolAction::TagItems { category_id, .. } => *category_id,
        _ => unreachable!(),
    };
    let fingerprint = action_fingerprint(command.internal_command_id, &action);
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
            let current = crate::category_store_read::collection_revision(transaction)?;
            let committed = next_core_revision(transaction, "read agent category core revision")?;
            let rejection = validate_action(transaction, category_id, &action)?;
            let disposition = rejection.map_or(RequestDisposition::Accepted, |reason| {
                RequestDisposition::Rejected { reason }
            });
            let episodes = episode_ids(transaction, &action)?;
            let completion = if disposition == RequestDisposition::Accepted {
                AgentToolCompletion::CategoryChanged { category_id }
            } else {
                AgentToolCompletion::Failed { code: 1 }
            };
            plan_agent_artifact_handoff(AgentArtifactHandoffActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                turn_id,
                subject: ActivitySubject::Operation {
                    command_id: CommandId::from_bytes(command.internal_command_id.into_bytes()),
                },
                episode_ids: episodes,
                transition: UserArtifactTransition::CategoryChanged,
                completion,
                current_revision: current,
                committed_revision: if disposition == RequestDisposition::Accepted {
                    committed
                } else {
                    current
                },
                disposition,
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, committed)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, (mutation, committed)| match mutation {
            AgentArtifactMutation::Apply => {
                require_revision(transaction, expected)?;
                let actual = apply(
                    transaction,
                    command.internal_command_id,
                    &fingerprint_text,
                    category_id,
                    &action,
                    observed_at.value,
                )?;
                (actual == committed)
                    .then_some(actual)
                    .ok_or(StorageError::RevisionConflict)
            }
            AgentArtifactMutation::None => {
                require_revision(transaction, expected)?;
                Ok(expected)
            }
        },
    )?;
    Ok(category_id)
}

fn validate_action(
    transaction: &rusqlite::Transaction<'_>,
    category_id: CategoryId,
    action: &AgentToolAction,
) -> Result<Option<RequestRejectionReason>, StorageError> {
    if validate_agent_action(action).is_err() {
        return Ok(Some(RequestRejectionReason::Invalid));
    }
    match action {
        AgentToolAction::WriteCategory { category_id: None, name, description, color_hex, .. } => {
            let valid = pod0_domain::validate_category(
                name.as_deref().unwrap_or_default(), description.as_deref().unwrap_or_default(),
                color_hex.as_deref(), CategoryOrigin::Agent,
            ).is_ok() && crate::category_store_read::active_category_count(transaction)? < pod0_domain::MAX_CATEGORIES;
            Ok((!valid).then_some(RequestRejectionReason::Invalid))
        }
        AgentToolAction::WriteCategory { category_id: Some(_), .. } => Ok(
            (!crate::category_store_read::category_exists(transaction, category_id)?)
                .then_some(RequestRejectionReason::MissingSubject),
        ),
        AgentToolAction::TagItems { add_item_ids, .. } => {
            if !crate::category_store_read::category_exists(transaction, category_id)? {
                return Ok(Some(RequestRejectionReason::MissingSubject));
            }
            for item in add_item_ids {
                if resolve_item(transaction, *item)?.is_none() {
                    return Ok(Some(RequestRejectionReason::MissingSubject));
                }
            }
            Ok(None)
        }
        _ => Err(StorageError::InvalidActivity),
    }
}

fn episode_ids(
    transaction: &rusqlite::Transaction<'_>,
    action: &AgentToolAction,
) -> Result<Vec<EpisodeId>, StorageError> {
    let AgentToolAction::TagItems { add_item_ids, remove_item_ids, .. } = action else {
        return Ok(Vec::new());
    };
    let mut episodes = Vec::new();
    for item in add_item_ids.iter().chain(remove_item_ids) {
        if resolve_item(transaction, *item)? == Some(CategoryItemKind::Episode) {
            episodes.push(EpisodeId::from_bytes(item.into_bytes()));
        }
    }
    Ok(episodes)
}

include!("transition_commit_agent_category_write.rs");
