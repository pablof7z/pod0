use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedInternalCommand, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, NonEmptyActivityFacts, PlaybackTransition, RequestDisposition,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackActivityInput {
    pub command_id: CommandId,
    pub episode_id: Option<EpisodeId>,
    pub current_revision: StateRevision,
    pub legacy_command_revision: Option<StateRevision>,
    pub transition: PlaybackTransition,
    pub internal_command: Option<DurableInternalCommandRequest>,
}

pub type PlaybackActivityPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_playback_activity(
    input: PlaybackActivityInput,
) -> Result<PlaybackActivityPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let subject = input
        .episode_id
        .map_or(ActivitySubject::Global, |episode_id| {
            ActivitySubject::Episode { episode_id }
        });
    let disposition = if input.legacy_command_revision.is_some() {
        RequestDisposition::Duplicate
    } else {
        RequestDisposition::Accepted
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        // The current command envelope has no trusted caller provenance. Do
        // not invent a user/agent attribution; the playback machine is the
        // durable actor until command provenance becomes typed at ingress.
        actor: ActivityActor::System,
        origin: ActivityOrigin::Playback,
        subject,
        episode_id: input.episode_id,
        fact,
    };
    let head = base(0, ActivityFact::RequestDisposition { disposition });
    let mut internal_commands = Vec::new();
    let facts = if disposition == RequestDisposition::Accepted {
        let committed_revision = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        let mut tail = vec![base(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Playback(input.transition),
                previous_revision: input.current_revision,
                committed_revision,
            },
        )];
        if let Some(command) = input.internal_command {
            let internal_command_id = identity.internal_command_id(0);
            tail.push(base(
                2,
                ActivityFact::InternalCommandAuthorized {
                    internal_command_id,
                    target: command.target,
                },
            ));
            internal_commands.push(AuthorizedInternalCommand {
                internal_command_id,
                authorizing_fact_index: 2,
                command,
            });
        }
        NonEmptyActivityFacts::from_head_and_tail(head, tail)
    } else {
        NonEmptyActivityFacts::new(head)
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        facts,
        Vec::new(),
        internal_commands,
    )
}
