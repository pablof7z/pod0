use pod0_domain::{CommandId, StateRevision};
use sha2::{Digest as _, Sha256};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError, UserArtifactTransition,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserArtifactMigrationActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub transition: UserArtifactTransition,
    pub disposition: RequestDisposition,
    pub authority_cutover: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserArtifactMigrationMutation {
    Apply,
    None,
}

pub type UserArtifactMigrationPlan = TransitionPlan<
    UserArtifactMigrationMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

#[must_use]
pub fn user_artifact_migration_command_id(
    domain: &str,
    operation: &str,
    source_id: CommandId,
) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-user-artifact-migration-command-v1");
    hash.update((domain.len() as u64).to_be_bytes());
    hash.update(domain.as_bytes());
    hash.update((operation.len() as u64).to_be_bytes());
    hash.update(operation.as_bytes());
    hash.update(source_id.into_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    CommandId::from_bytes(digest[..16].try_into().expect("digest prefix"))
}

pub fn plan_user_artifact_migration(
    input: UserArtifactMigrationActivityInput,
) -> Result<UserArtifactMigrationPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::Migration,
        origin: ActivityOrigin::Migration,
        subject: ActivitySubject::Global,
        episode_id: None,
        fact,
    };
    let accepted = input.disposition == RequestDisposition::Accepted;
    if accepted && input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let disposition = base(
        0,
        ActivityFact::RequestDisposition {
            disposition: input.disposition,
        },
    );
    let facts = if accepted {
        let mut tail = vec![base(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::UserArtifact(input.transition),
                previous_revision: input.current_revision,
                committed_revision: input.committed_revision,
            },
        )];
        if input.authority_cutover {
            tail.push(base(
                2,
                ActivityFact::AuthorityCutover {
                    domain: ActivityDomain::UserArtifact,
                },
            ));
        }
        NonEmptyActivityFacts::from_head_and_tail(disposition, tail)
    } else {
        NonEmptyActivityFacts::new(disposition)
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if accepted {
            UserArtifactMigrationMutation::Apply
        } else {
            UserArtifactMigrationMutation::None
        },
        facts,
        Vec::new(),
        Vec::new(),
    )
}
